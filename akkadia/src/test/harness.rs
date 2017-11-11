// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use lstypes;
use json;
use server as ls_server;
use vfs;

use init_logger;

const TEST_TIMEOUT_IN_SEC: u64 = 320;

pub struct Environment {
    pub cache: Cache,
    pub target_path: PathBuf,
}

impl Environment {
    pub fn new(project_dir: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};

        lazy_static! {
            static ref COUNTER: AtomicUsize = AtomicUsize::new(0);
        }

        init_logger();

        let cur_dir = env::current_dir().expect("Could not find current working directory");
        let project_path = cur_dir.join("test_data").join(project_dir);
        let target_path = cur_dir.join("target").join("tests").join(format!(
            "{}",
            COUNTER.fetch_add(
                1,
                Ordering::Relaxed,
            )
        ));

        let cache = Cache::new(project_path);

        Self { cache, target_path }
    }
}

impl Environment {
    // Initialise and run the internals of an LS protocol RLS server.
    pub fn mock_server(
        &mut self,
        messages: Vec<String>,
    ) -> (ls_server::LsService<RecordOutput>, LsResultList) {
        let vfs = Arc::new(vfs::Vfs::new());
        let reader = Box::new(MockMsgReader::new(messages));
        let output = RecordOutput::new();
        let results = output.output.clone();
        (ls_server::LsService::new(vfs, reader, output), results)
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        use std::fs;

        if fs::metadata(&self.target_path).is_ok() {
            fs::remove_dir_all(&self.target_path).expect("failed to tidy up");
        }
    }
}

struct MockMsgReader {
    messages: Vec<String>,
    cur: Mutex<usize>,
}

impl MockMsgReader {
    fn new(messages: Vec<String>) -> MockMsgReader {
        MockMsgReader {
            messages: messages,
            cur: Mutex::new(0),
        }
    }
}

impl ls_server::MessageReader for MockMsgReader {
    fn read_message(&self) -> Option<String> {
        // Note that we hold this lock until the end of the function, thus meaning
        // that we must finish processing one message before processing the next.
        let mut cur = self.cur.lock().unwrap();
        let index = *cur;
        *cur += 1;

        if index >= self.messages.len() {
            return None;
        }

        let message = &self.messages[index];

        Some(message.to_owned())
    }
}

type LsResultList = Arc<Mutex<Vec<String>>>;

#[derive(Clone)]
pub struct RecordOutput {
    pub output: LsResultList,
}

impl RecordOutput {
    pub fn new() -> RecordOutput {
        RecordOutput { output: Arc::new(Mutex::new(vec![])) }
    }
}

impl ls_server::Output for RecordOutput {
    fn response(&self, output: String) {
        // Ignore server -> client requests
        let value: json::Value = json::from_str(&output).unwrap();
        if let Some(id) = value.get("id") {
            if let Some(id) = id.as_u64() {
                if id as u32 == 0xDEADBEEF {
                    return;
                }
            }
        }

        debug!("response: {:?}", output);

        let mut records = self.output.lock().unwrap();
        records.push(output);
    }

    fn provide_id(&self) -> u32 {
        0xDEADBEEF
    }
}

#[derive(Clone, Debug)]
pub struct ExpectedMessage {
    id: Option<u64>,
    contains: Vec<String>,
}

impl ExpectedMessage {
    pub fn new(id: Option<u64>) -> ExpectedMessage {
        ExpectedMessage {
            id: id,
            contains: vec![],
        }
    }

    pub fn expect_contains(&mut self, s: &str) -> &mut ExpectedMessage {
        self.contains.push(s.to_owned());
        self
    }
}

pub fn expect_messages(results: LsResultList, expected: &[&ExpectedMessage]) {
    let start_clock = SystemTime::now();
    let mut results_count = results.lock().unwrap().len();
    while results_count != expected.len() {
        if start_clock.elapsed().unwrap().as_secs() >= TEST_TIMEOUT_IN_SEC {
            panic!("Hit timeout");
        }
        thread::sleep(Duration::from_millis(100));
        results_count = results.lock().unwrap().len();
    }

    let mut results = results.lock().unwrap();

    println!(
        "expect_messages:\n  results: {:#?},\n  expected: {:#?}",
        *results,
        expected
    );
    assert_eq!(results.len(), expected.len());
    for (found, expected) in results.iter().zip(expected.iter()) {
        let values: json::Value = json::from_str(found).unwrap();
        assert!(
            values
                .get("jsonrpc")
                .expect("Missing jsonrpc field")
                .as_str()
                .unwrap() == "2.0",
            "Bad jsonrpc field"
        );
        if let Some(id) = expected.id {
            assert_eq!(
                values
                    .get("id")
                    .expect("Missing id field")
                    .as_u64()
                    .unwrap(),
                id,
                "Unexpected id"
            );
        }
        for c in expected.contains.iter() {
            found.find(c).expect(&format!(
                "Could not find `{}` in `{}`",
                c,
                found
            ));
        }
    }

    *results = vec![];
}

#[derive(Clone, Copy, Debug)]
pub struct Src<'a, 'b> {
    pub file_name: &'a Path,
    // 1 indexed
    pub line: usize,
    pub name: &'b str,
}

pub fn src<'a, 'b>(file_name: &'a Path, line: usize, name: &'b str) -> Src<'a, 'b> {
    Src {
        file_name: file_name,
        line: line,
        name: name,
    }
}

pub struct Cache {
    base_path: PathBuf,
    files: HashMap<PathBuf, Vec<String>>,
}

impl Cache {
    fn new(base_path: PathBuf) -> Cache {
        Cache {
            base_path,
            files: HashMap::new(),
        }
    }

    pub fn mk_ls_position(&mut self, src: Src) -> lstypes::Position {
        let line = self.get_line(src);
        let col = line.find(src.name).expect(&format!(
            "Line does not contain name {}",
            src.name
        ));
        lstypes::Position::new((src.line - 1) as u64, char_of_byte_index(&line, col) as u64)
    }

    pub fn abs_path(&self, file_name: &Path) -> PathBuf {
        let result = self.base_path.join(file_name).canonicalize().expect(
            "Couldn't canonicalise path",
        );
        let result = if cfg!(windows) {
            // FIXME: If the \\?\ prefix is not stripped from the canonical path, the HTTP server tests fail. Why?
            let result_string = result.to_str().expect("Path contains non-utf8 characters.");
            PathBuf::from(&result_string[r"\\?\".len()..])
        } else {
            result
        };
        result
    }

    fn get_line(&mut self, src: Src) -> String {
        let base_path = &self.base_path;
        let lines = self.files.entry(src.file_name.to_owned()).or_insert_with(
            || {
                let file_name = &base_path.join(src.file_name);
                let file =
                    File::open(file_name).expect(&format!("Couldn't find file: {:?}", file_name));
                let lines = BufReader::new(file).lines();
                lines.collect::<Result<Vec<_>, _>>().unwrap()
            },
        );

        if src.line - 1 >= lines.len() {
            panic!("Line {} not in file, found {} lines", src.line, lines.len());
        }

        lines[src.line - 1].to_owned()
    }
}

fn char_of_byte_index(s: &str, byte: usize) -> usize {
    for (c, (b, _)) in s.char_indices().enumerate() {
        if b == byte {
            return c;
        }
    }

    panic!("Couldn't find byte {} in {:?}", byte, s);
}
