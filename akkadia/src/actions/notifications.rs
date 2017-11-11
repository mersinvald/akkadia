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
use vfs::Change;
use serde::Deserialize;
use serde::de::Error;
use json;
use lsp_data::Span;

use lsp_data::*;
use server::{Output, Action, NotificationAction, LsState, NoParams};

use std::thread;

#[derive(Debug, PartialEq)]
pub struct Initialized;

impl<'a> Action<'a> for Initialized {
    type Params = NoParams;
    const METHOD: &'static str = "initialized";

    fn new(_: &'a mut LsState) -> Self {
        Initialized
    }
}

impl<'a> NotificationAction<'a> for Initialized {
    // Respond to the `initialized` notification. We take this opportunity to
    // dynamically register some options.
    fn handle<O: Output>(
        &mut self,
        _params: Self::Params,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct DidOpen;

impl<'a> Action<'a> for DidOpen {
    type Params = DidOpenTextDocumentParams;
    const METHOD: &'static str = "textDocument/didOpen";

    fn new(_: &'a mut LsState) -> Self {
        DidOpen
    }
}

impl<'a> NotificationAction<'a> for DidOpen {
    fn handle<O: Output>(
        &mut self,
        params: Self::Params,
        ctx: &mut ActionContext,
        _out: O,
    ) -> Result<(), ()> {
        trace!("on_open: {:?}", params.text_document.uri);
        let ctx = ctx.inited();

        let file_path = parse_file_path!(&params.text_document.uri, "on_open")?;

        ctx.vfs.set_file(&file_path, &params.text_document.text);
        Ok(())
    }
}

#[derive(Debug)]
pub struct DidChange;

impl<'a> Action<'a> for DidChange {
    type Params = DidChangeTextDocumentParams;
    const METHOD: &'static str = "textDocument/didChange";

    fn new(_: &'a mut LsState) -> Self {
        DidChange
    }
}

impl<'a> NotificationAction<'a> for DidChange {
    fn handle<O: Output>(
        &mut self,
        params: Self::Params,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        trace!(
            "on_change: {:?}, thread: {:?}",
            params,
            thread::current().id()
        );

        let ctx = ctx.inited();

        let file_path = parse_file_path!(&params.text_document.uri, "on_change")?;

        let changes: Vec<Change> = params
            .content_changes
            .iter()
            .map(|i| if let Some(range) = i.range {
                let range = ls_util::range_to_span(range);
                Change::ReplaceText {
                    span: Span::from_range(range, file_path.clone()),
                    len: i.range_length,
                    text: i.text.clone(),
                }
            } else {
                Change::AddFile {
                    file: file_path.clone(),
                    text: i.text.clone(),
                }
            })
            .collect();
        ctx.vfs.on_changes(&changes).expect(
            "error committing to VFS",
        );
        Ok(())
    }
}

#[derive(Debug)]
pub struct Cancel;

impl<'a> Action<'a> for Cancel {
    type Params = CancelParams;
    const METHOD: &'static str = "$/cancelRequest";

    fn new(_: &'a mut LsState) -> Self {
        Cancel
    }
}

impl<'a> NotificationAction<'a> for Cancel {
    fn handle<O: Output>(
        &mut self,
        _params: CancelParams,
        _ctx: &mut ActionContext,
        _out: O,
    ) -> Result<(), ()> {
        // Nothing to do.
        Ok(())
    }
}

#[derive(Debug)]
pub struct DidSave;

impl<'a> Action<'a> for DidSave {
    type Params = DidSaveTextDocumentParams;
    const METHOD: &'static str = "textDocument/didSave";

    fn new(_: &'a mut LsState) -> Self {
        DidSave
    }
}

impl<'a> NotificationAction<'a> for DidSave {
    fn handle<O: Output>(
        &mut self,
        params: DidSaveTextDocumentParams,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        let ctx = ctx.inited();
        
        let file_path = parse_file_path!(&params.text_document.uri, "on_save")?;

        ctx.vfs.file_saved(&file_path).unwrap();

        Ok(())
    }
}

#[derive(Debug)]
pub struct DidChangeWatchedFiles;

impl<'a> Action<'a> for DidChangeWatchedFiles {
    type Params = DidChangeWatchedFilesParams;
    const METHOD: &'static str = "workspace/didChangeWatchedFiles";

    fn new(_: &'a mut LsState) -> Self {
        DidChangeWatchedFiles
    }
}

impl<'a> NotificationAction<'a> for DidChangeWatchedFiles {
    fn handle<O: Output>(
        &mut self,
        _params: DidChangeWatchedFilesParams,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        Ok(())
    }
}
