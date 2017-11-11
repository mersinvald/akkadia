// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Utilities and infrastructure for testing. Tests in this module test the
// testing infrastructure *not* the RLS.

mod harness;

use actions::requests;
use server::{self as ls_server, Request};
use jsonrpc;
use vfs;

use self::harness::{Environment, expect_messages, ExpectedMessage, RecordOutput, src};

use lstypes::*;
use lsp_data::InitializationOptions;

use json;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Arc, Mutex};
use url::Url;

pub fn initialize<'a>(
    id: usize,
    root_path: Option<String>,
) -> Request<'a, ls_server::InitializeRequest> {
    initialize_with_opts(id, root_path, None)
}

pub fn initialize_with_opts<'a>(
    id: usize,
    root_path: Option<String>,
    initialization_options: Option<InitializationOptions>,
) -> Request<'a, ls_server::InitializeRequest> {
    let init_opts = initialization_options.map(|val| json::to_value(val).unwrap());
    let params = InitializeParams {
        process_id: None,
        root_path,
        root_uri: None,
        initialization_options: init_opts,
        capabilities: ClientCapabilities {
            workspace: None,
            text_document: None,
            experimental: None,
        },
        trace: TraceOption::Off,
    };
    Request {
        id,
        params,
        _action: PhantomData,
    }
}

pub fn request<'a, T: ls_server::RequestAction<'a>>(
    id: usize,
    params: T::Params,
) -> Request<'a, T> {
    Request {
        id,
        params,
        _action: PhantomData,
    }
}

#[test]
fn test_completion() {
    let mut env = Environment::new("common");

    let source_file_path = Path::new("src").join("main.slang");

    let root_path = env.cache.abs_path(Path::new("."));
    let url = Url::from_file_path(env.cache.abs_path(&source_file_path))
        .expect("couldn't convert file path to URL");
    let text_doc = TextDocumentIdentifier::new(url);

    let messages = vec![
        initialize(0, root_path.as_os_str().to_str().map(|x| x.to_owned()))
            .to_string(),
        request::<requests::Completion>(
            11,
            TextDocumentPositionParams {
                text_document: text_doc.clone(),
                position: env.cache.mk_ls_position(src(&source_file_path, 1, "Int")),
            }
        ).to_string(),
    ];

    let (mut server, results) = env.mock_server(messages);
    // Initialise and build.
    assert_eq!(
        ls_server::LsService::handle_message(&mut server),
        ls_server::ServerStateChange::Continue
    );
    expect_messages(
        results.clone(),
        &[
            ExpectedMessage::new(Some(0)).expect_contains("capabilities"),
        ],
    );

    assert_eq!(
        ls_server::LsService::handle_message(&mut server),
        ls_server::ServerStateChange::Continue
    );
    expect_messages(
        results.clone(),
        &[
            ExpectedMessage::new(Some(11)).expect_contains(
                r#"[{"label":"completion","detail":"test completion"}]"#,
            ),
        ],
    );
}
