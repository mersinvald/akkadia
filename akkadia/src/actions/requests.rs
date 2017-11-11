// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use actions::ActionContext;
use url::Url;
use vfs::FileContents;
use json;
use span;

use lsp_data;
use lsp_data::*;
use server::{Output, Ack, Action, RequestAction, LsState};
use jsonrpc::types::ErrorCode;

use std::collections::HashMap;
use std::panic;
use std::thread;
use std::time::Duration;

pub struct Completion;

impl<'a> Action<'a> for Completion {
    type Params = TextDocumentPositionParams;
    const METHOD: &'static str = "textDocument/completion";

    fn new(_: &'a mut LsState) -> Self {
        Completion
    }
}

impl<'a> RequestAction<'a> for Completion {
    type Response = Vec<CompletionItem>;
    fn handle<O: Output>(
        &mut self,
        _id: usize,
        params: Self::Params,
        ctx: &mut ActionContext,
        _out: O,
    ) -> Result<Self::Response, ()> {
        let ctx = ctx.inited();
        let vfs = ctx.vfs.clone();
        let file_path = parse_file_path!(&params.text_document.uri, "complete")?;

        let result = vec![ 
            CompletionItem::new_simple("completion".to_owned(), "test completion".to_owned())
        ];

        Ok(result)
    }
}

pub struct ResolveCompletion;

impl<'a> Action<'a> for ResolveCompletion {
    type Params = CompletionItem;
    const METHOD: &'static str = "completionItem/resolve";

    fn new(_: &'a mut LsState) -> Self {
        ResolveCompletion
    }
}

impl<'a> RequestAction<'a> for ResolveCompletion {
    type Response = Vec<CompletionItem>;
    fn handle<O: Output>(
        &mut self,
        _id: usize,
        params: Self::Params,
        _ctx: &mut ActionContext,
        _out: O,
    ) -> Result<Self::Response, ()> {
        // currently, we safely ignore this as a pass-through since we fully handle
        // textDocument/completion.  In the future, we may want to use this method as a
        // way to more lazily fill out completion information
        Ok(vec![params])
    }
}
