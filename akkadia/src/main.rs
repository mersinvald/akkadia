#![feature(integer_atomics)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate log;
extern crate fern;
extern crate chrono;

#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate serde_json as json;

extern crate jsonrpc_core as jsonrpc;
extern crate languageserver_types as lstypes;
extern crate url;

extern crate akkadia_span as span;
extern crate akkadia_vfs as vfs;

mod lsp_data;
mod server;
mod test;
mod actions;

static AKKADIA_LOG_FILE: &str = ".akkadia.log";

use vfs::Vfs;
use std::sync::Arc;

fn main() {
    init_logger().unwrap();

    let vfs = Arc::new(Vfs::new());

    server::run_server(vfs);
}

use std::io::Write;
use std::io::stderr;
use std::fs::OpenOptions;

fn init_logger() -> Result<(), log::SetLoggerError> {
    // Output logs to stderr by default
    let mut fern_dispatch = fern::Dispatch::default()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}:{}][{}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                record.target(),
                record.location().line(),
                record.level(),
                message
            ))
        })
        .level(log::LogLevelFilter::Trace)
        .chain(stderr());

    // Try to open log file
    let log_file = OpenOptions::new().append(true).create(true).open(
        AKKADIA_LOG_FILE,
    );

    // Chain up log file if it was opened successfuly
    match log_file {
        Ok(file) => fern_dispatch = fern_dispatch.chain(file),
        Err(err) => {
            writeln!(
                stderr(),
                "Couldn't open log file {}: {}",
                AKKADIA_LOG_FILE,
                err
            ).unwrap()
        }
    }

    // Apply logging configuration
    fern_dispatch.apply()
}

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
