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
use std::fmt::{self, Debug};
use std::path::PathBuf;
use std::error::Error;

use url::Url;
use serde::Serialize;
use span;
use vfs::FileContents;

pub use lstypes::*;
use jsonrpc::version;


pub type Span = span::Span<span::ZeroIndexed>;
pub const NOTIFICATION_DIAGNOSTICS_BEGIN: &'static str = "akkadiaDocument/diagnosticsBegin";
pub const NOTIFICATION_DIAGNOSTICS_END: &'static str = "akkadiaDocument/diagnosticsEnd";
pub const NOTIFICATION_BUILD_BEGIN: &'static str = "akkadiaDocument/beginBuild";

#[derive(Debug)]
pub enum UrlFileParseError {
    InvalidScheme,
    InvalidFilePath,
}

impl Error for UrlFileParseError {
    fn description(&self) -> &str {
        match *self {
            UrlFileParseError::InvalidScheme => "URI scheme is not `file`",
            UrlFileParseError::InvalidFilePath => "Invalid file path in URI",
        }
    }
}

impl fmt::Display for UrlFileParseError
where
    UrlFileParseError: Error,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

pub fn parse_file_path(uri: &Url) -> Result<PathBuf, UrlFileParseError> {
    if uri.scheme() != "file" {
        Err(UrlFileParseError::InvalidScheme)
    } else {
        uri.to_file_path().map_err(
            |_err| UrlFileParseError::InvalidFilePath,
        )
    }
}

pub fn make_workspace_edit(location: Location, new_text: String) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit { changes: HashMap::new() };

    edit.changes.insert(
        location.uri,
        vec![
            TextEdit {
                range: location.range,
                new_text,
            },
        ],
    );

    edit
}

pub mod ls_util {
    use super::*;

    use std::path::Path;
    use vfs::Vfs;

    pub fn range_to_span(r: Range) -> span::Range<span::ZeroIndexed> {
        span::Range::from_positions(position_to_span(r.start), position_to_span(r.end))
    }

    pub fn position_to_span(p: Position) -> span::Position<span::ZeroIndexed> {
        span::Position::new(
            span::Row::new_zero_indexed(p.line as u32),
            span::Column::new_zero_indexed(p.character as u32),
        )
    }

    pub fn location_to_span(
        l: Location,
    ) -> Result<span::Span<span::ZeroIndexed>, UrlFileParseError> {
        parse_file_path(&l.uri).map(|path| Span::from_range(range_to_span(l.range), path))
    }

    // An RLS span has the same info as an LSP Location
    pub fn span_to_location(span: &Span) -> Location {
        Location {
            uri: Url::from_file_path(&span.file).unwrap(),
            range: span_to_range(span.range),
        }
    }

    pub fn span_location_to_location(l: &span::Location<span::ZeroIndexed>) -> Location {
        Location {
            uri: Url::from_file_path(&l.file).unwrap(),
            range: span_to_range(span::Range::from_positions(l.position, l.position)),
        }
    }

    pub fn span_to_range(r: span::Range<span::ZeroIndexed>) -> Range {
        Range {
            start: span_to_position(r.start()),
            end: span_to_position(r.end()),
        }
    }

    pub fn span_to_position(p: span::Position<span::ZeroIndexed>) -> Position {
        Position {
            line: p.row.0 as u64,
            character: p.col.0 as u64,
        }
    }

    /// Creates a `Range` spanning the whole file as currently known by `Vfs`
    ///
    /// Panics if `Vfs` cannot load the file.
    pub fn range_from_vfs_file(vfs: &Vfs, fname: &Path) -> Range {
        // FIXME load_file clones the entire file text, this could be much more
        // efficient by adding a `with_file` fn to the VFS.
        let content = match vfs.load_file(fname).unwrap() {
            FileContents::Text(t) => t,
            _ => panic!("unexpected binary file: {:?}", fname),
        };
        if content.is_empty() {
            Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            }
        } else {
            let mut line_count = content.lines().count() as u64 - 1;
            let col = if content.ends_with('\n') {
                line_count += 1;
                0
            } else {
                content
                    .lines()
                    .last()
                    .expect("String is not empty.")
                    .chars()
                    .count() as u64
            };
            // range is zero-based and the end position is exclusive
            Range {
                start: Position::new(0, 0),
                end: Position::new(line_count, col),
            }
        }
    }
}

/* -----------------  JSON-RPC protocol types ----------------- */

/// Supported initilization options that can be passed in the `initialize`
/// request, under `initialization_options` key. These are specific to the RLS.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct InitializationOptions {
    /// Should the build not be triggered immediately after receiving `initialize`
    #[serde(rename = "omitInitBuild")]
    pub omit_init_build: bool,
}

impl Default for InitializationOptions {
    fn default() -> Self {
        InitializationOptions { omit_init_build: false }
    }
}

/// An event-like (no response needed) notification message.
#[derive(Debug, Serialize)]
pub struct NotificationMessage {
    jsonrpc: version::Version,
    pub method: &'static str,
    pub params: Option<PublishDiagnosticsParams>,
}

impl NotificationMessage {
    pub fn new(method: &'static str, params: Option<PublishDiagnosticsParams>) -> Self {
        NotificationMessage {
            jsonrpc: version::Version::V2,
            method,
            params,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RequestMessage<T>
where
    T: Debug + Serialize,
{
    jsonrpc: &'static str,
    pub id: u32,
    pub method: String,
    pub params: T,
}

impl<T> RequestMessage<T>
where
    T: Debug + Serialize,
{
    pub fn new(id: u32, method: String, params: T) -> Self {
        RequestMessage {
            jsonrpc: "2.0",
            id,
            method: method,
            params: params,
        }
    }
}
