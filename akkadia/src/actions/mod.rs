// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use vfs::Vfs;
use span;
use lsp_data::Span;
use lsp_data::*;
use server::Output;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;


// TODO: Support non-`file` URI schemes in VFS. We're currently ignoring them because
// we don't want to crash the RLS in case a client opens a file under different URI scheme
// like with git:/ or perforce:/ (Probably even http:/? We currently don't support remote schemes).
macro_rules! ignore_non_file_uri {
    ($expr: expr, $uri: expr, $log_name: expr) => {
        $expr.map_err(|_| {
            trace!("{}: Non-`file` URI scheme, ignoring: {:?}", $log_name, $uri);
            ()
        })
    };
}

macro_rules! parse_file_path {
    ($uri: expr, $log_name: expr) => {
        ignore_non_file_uri!(parse_file_path($uri), $uri, $log_name)
    }
}

pub mod requests;
pub mod notifications;

pub enum ActionContext {
    Init(InitActionContext),
    Uninit(UninitActionContext),
}

impl ActionContext {
    pub fn new(vfs: Arc<Vfs>) -> ActionContext {
        ActionContext::Uninit(UninitActionContext::new(vfs))
    }

    pub fn init<O: Output>(&mut self, current_project: PathBuf, init_options: &InitializationOptions, out: O) {
        let ctx = match *self {
            ActionContext::Uninit(ref uninit) => {
                InitActionContext::new(uninit.vfs.clone(), current_project)
            }
            ActionContext::Init(_) => panic!("ActionContext already initialized"),
        };
        *self = ActionContext::Init(ctx);
    }

    fn inited(&self) -> &InitActionContext {
        match *self {
            ActionContext::Uninit(_) => panic!("ActionContext not initialized"),
            ActionContext::Init(ref ctx) => ctx,
        }
    }
}

pub struct InitActionContext {
    vfs: Arc<Vfs>,
    current_project: PathBuf,
}

pub struct UninitActionContext {
    vfs: Arc<Vfs>,
}

impl UninitActionContext {
    fn new(vfs: Arc<Vfs>) -> UninitActionContext {
        UninitActionContext {
            vfs,
        }
    }

}

impl InitActionContext {
    fn new(vfs: Arc<Vfs>,
           current_project: PathBuf) -> InitActionContext {
        InitActionContext {
            vfs,
            current_project,
        }
    }

    fn convert_pos_to_span(&self, file_path: PathBuf, pos: Position) -> Span {
        trace!("convert_pos_to_span: {:?} {:?}", file_path, pos);

        let pos = ls_util::position_to_span(pos);
        let line = self.vfs.load_line(&file_path, pos.row).unwrap();
        trace!("line: `{}`", line);

        let (start, end) = find_word_at_pos(&line, &pos.col);
        trace!("start: {}, end: {}", start.0, end.0);

        Span::from_positions(span::Position::new(pos.row, start),
                             span::Position::new(pos.row, end),
                             file_path)
    }
}

/// Represents a text cursor between characters, pointing at the next character
/// in the buffer.
type Column = span::Column<span::ZeroIndexed>;
/// Returns a text cursor range for a found word inside `line` at which `pos`
/// text cursor points to. Resulting type represents a (`start`, `end`) range
/// between `start` and `end` cursors.
/// For example (4, 4) means an empty selection starting after first 4 characters.
fn find_word_at_pos(line: &str, pos: &Column) -> (Column, Column) {
    let col = pos.0 as usize;
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';

    let start = line.chars()
        .enumerate()
        .take(col)
        .filter(|&(_, c)| !is_ident_char(c))
        .last()
        .map(|(i, _)| i + 1)
        .unwrap_or(0) as u32;

    let end = line.chars()
        .enumerate()
        .skip(col)
        .filter(|&(_, c)| !is_ident_char(c))
        .nth(0)
        .map(|(i, _)| i)
        .unwrap_or(col) as u32;

    (
        span::Column::new_zero_indexed(start),
        span::Column::new_zero_indexed(end),
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_find_word_at_pos() {
        fn assert_range(test_str: &'static str, range: (u32, u32)) {
            assert!(test_str.chars().filter(|c| *c == '|').count() == 1);
            let col = test_str.find('|').unwrap() as u32;
            let line = test_str.replace('|', "");
            let (start, end) = find_word_at_pos(&line, &Column::new_zero_indexed(col));
            assert_eq!(
                range,
                (start.0, end.0),
                "Assertion failed for {:?}",
                test_str
            );
        }

        assert_range("|struct Def {", (0, 6));
        assert_range("stru|ct Def {", (0, 6));
        assert_range("struct| Def {", (0, 6));

        assert_range("struct |Def {", (7, 10));
        assert_range("struct De|f {", (7, 10));
        assert_range("struct Def| {", (7, 10));

        assert_range("struct Def |{", (11, 11));

        assert_range("|span::Position<T>", (0, 4));
        assert_range(" |span::Position<T>", (1, 5));
        assert_range("sp|an::Position<T>", (0, 4));
        assert_range("span|::Position<T>", (0, 4));
        assert_range("span::|Position<T>", (6, 14));
        assert_range("span::Position|<T>", (6, 14));
        assert_range("span::Position<|T>", (15, 16));
        assert_range("span::Position<T|>", (15, 16));
    }
}
