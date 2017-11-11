// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use jsonrpc::{self, Id};
use vfs::Vfs;
use serde;
use json;
use serde::Deserialize;

use version;
use lsp_data::*;
use actions::ActionContext;
use actions::notifications;
use actions::requests;
pub use server::io::{MessageReader, Output};
use server::io::{StdioMsgReader, StdioOutput};

use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

mod io;

pub fn run_server(vfs: Arc<Vfs>) {
    debug!(
        "Akkadia Language Server starting up. Version: {}",
        version()
    );

    let service = LsService::new(vfs, Box::new(StdioMsgReader), StdioOutput::new());
    service.run();

    debug!("Akkadia Language Server shutting down");
}

#[derive(Debug, Serialize)]
pub struct Ack;

#[derive(Debug, Serialize, PartialEq)]
pub struct NoParams;

impl<'de> Deserialize<'de> for NoParams {
    fn deserialize<D>(_deserializer: D) -> Result<NoParams, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(NoParams)
    }
}

pub trait Action<'a> {
    type Params: serde::Serialize + for<'de> ::serde::Deserialize<'de>;
    const METHOD: &'static str;

    fn new(state: &'a mut LsState) -> Self;
}

pub trait NotificationAction<'a>: Action<'a> {
    fn handle<O: Output>(
        &mut self,
        params: Self::Params,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()>;
}

pub trait RequestAction<'a>: Action<'a> {
    type Response: ::serde::Serialize + fmt::Debug;

    fn handle<O: Output>(
        &mut self,
        id: usize,
        params: Self::Params,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<Self::Response, ()>;
}


pub struct Request<'a, A: RequestAction<'a>> {
    pub id: usize,
    pub params: A::Params,
    pub _action: PhantomData<A>,
}

#[derive(Debug, PartialEq)]
pub struct Notification<'a, A: NotificationAction<'a>> {
    pub params: A::Params,
    pub _action: PhantomData<A>,
}

impl<'a, A: RequestAction<'a>> Request<'a, A> {
    fn dispatch<O: Output>(
        self,
        state: &'a mut LsState,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<A::Response, ()> {
        let mut action = A::new(state);
        let result = action.handle(self.id, self.params, ctx, out.clone())?;
        if ::std::mem::size_of::<A::Response>() > 0 {
            out.success(self.id, &result);
        }
        Ok(result)
    }
}

impl<'a, A: NotificationAction<'a>> Notification<'a, A> {
    fn dispatch<O: Output>(
        self,
        state: &'a mut LsState,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        let mut action = A::new(state);
        action.handle(self.params, ctx, out)?;
        Ok(())
    }
}

impl<'a, A: RequestAction<'a>> fmt::Display for Request<'a, A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        json!({
            "jsonrpc": "2.0",
            "id": self.id,
            "method": A::METHOD,
            "params": self.params,
        }).fmt(f)
    }
}

impl<'a, A: NotificationAction<'a>> fmt::Display for Notification<'a, A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        json!({
            "jsonrpc": "2.0",
            "method": A::METHOD,
            "params": self.params,
        }).fmt(f)
    }
}

pub struct LsService<O: Output> {
    msg_reader: Box<MessageReader + Send + Sync>,
    output: O,
    pub ctx: ActionContext,
    pub state: LsState,
}

#[derive(Debug)]
pub struct LsState {
    shut_down: AtomicBool,
}

pub struct ShutdownRequest<'a> {
    state: &'a mut LsState,
}

impl<'a> Action<'a> for ShutdownRequest<'a> {
    type Params = NoParams;
    const METHOD: &'static str = "shutdown";

    fn new(state: &'a mut LsState) -> Self {
        ShutdownRequest { state }
    }
}

impl<'a> RequestAction<'a> for ShutdownRequest<'a> {
    type Response = Ack;
    fn handle<O: Output>(
        &mut self,
        _id: usize,
        _params: Self::Params,
        ctx: &mut ActionContext,
        _out: O,
    ) -> Result<Self::Response, ()> {
        self.state.shut_down.store(true, Ordering::SeqCst);
        Ok(Ack)
    }
}

#[derive(Debug)]
pub struct ExitNotification<'a> {
    state: &'a mut LsState,
}

impl<'a> Action<'a> for ExitNotification<'a> {
    type Params = NoParams;
    const METHOD: &'static str = "exit";

    fn new(state: &'a mut LsState) -> Self {
        ExitNotification { state }
    }
}

impl<'a> NotificationAction<'a> for ExitNotification<'a> {
    fn handle<O: Output>(
        &mut self,
        _params: Self::Params,
        _ctx: &mut ActionContext,
        _out: O,
    ) -> Result<(), ()> {
        let shut_down = self.state.shut_down.load(Ordering::SeqCst);
        ::std::process::exit(if shut_down { 0 } else { 1 });
    }
}

pub struct InitializeRequest;

impl<'a> Action<'a> for InitializeRequest {
    type Params = InitializeParams;
    const METHOD: &'static str = "initialize";

    fn new(_: &'a mut LsState) -> Self {
        InitializeRequest
    }
}

impl<'a> RequestAction<'a> for InitializeRequest {
    type Response = ();
    fn handle<O: Output>(
        &mut self,
        id: usize,
        params: Self::Params,
        ctx: &mut ActionContext,
        out: O,
    ) -> Result<(), ()> {
        let init_options: InitializationOptions = params
            .initialization_options
            .as_ref()
            .and_then(|options| json::from_value(options.to_owned()).ok())
            .unwrap_or_default();

        trace!("init: {:?}", init_options);

        let result = InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncKind::Incremental),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(true),
                    trigger_characters: vec![".".to_string(), ":".to_string()],
                }),
                ..ServerCapabilities::default()
            }
        };

        trace!("capabilities: {:#?}", result);

        out.success(id, &result);

        trace!("responded");

        let root_path = params.root_path.as_ref().map(PathBuf::from).expect("No root path");
        ctx.init(root_path, &init_options, out);

        Ok(())
    }
}


#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ServerStateChange {
    Continue,
    Break,
}

impl<O: Output> LsService<O> {
    pub fn new(vfs: Arc<Vfs>, reader: Box<MessageReader + Send + Sync>, output: O) -> LsService<O> {
        let ctx = ActionContext::new(vfs.clone());
        LsService {
            msg_reader: reader,
            output: output,
            ctx: ctx,
            state: LsState { shut_down: AtomicBool::new(false) },
        }
    }

    pub fn run(mut self) {
        while self.handle_message() == ServerStateChange::Continue {}
    }

    fn parse_message(&mut self, msg: &str) -> Result<Option<RawMessage>, jsonrpc::Error> {
        // Parse the message.
        let ls_command: json::Value = json::from_str(msg).map_err(
            |_| jsonrpc::Error::parse_error(),
        )?;

        // Per JSON-RPC/LSP spec, Requests must have id, whereas Notifications can't
        let id = ls_command.get("id").map(|id| {
            json::from_value(id.to_owned()).unwrap()
        });

        let method = match ls_command.get("method") {
            Some(method) => method,
            // No method means this is a response to one of our requests. FIXME: we should
            // confirm these, but currently just ignore them.
            None => return Ok(None),
        };

        let method = method
            .as_str()
            .ok_or_else(|| jsonrpc::Error::invalid_request())?
            .to_owned();

        // Representing internally a missing parameter as Null instead of None,
        // (Null being unused value of param by the JSON-RPC 2.0 spec)
        // to unify the type handling – now the parameter type implements Deserialize.
        let params = match ls_command.get("params").map(|p| p.to_owned()) {
            Some(params @ json::Value::Object(..)) => params,
            Some(params @ json::Value::Array(..)) => params,
            None => json::Value::Null,
            // Null as input value is not allowed by JSON-RPC 2.0,
            //but including it for robustness
            Some(json::Value::Null) => json::Value::Null,
            _ => return Err(jsonrpc::Error::invalid_request()),
        };

        Ok(Some(RawMessage { method, id, params }))
    }

    fn dispatch_message(&mut self, msg: &RawMessage) -> Result<(), jsonrpc::Error> {
        macro_rules! match_action {
            ($method: expr; notifications: $($n_action: ty),*; requests: $($r_action: ty),*;) => {
                let mut handled = false;
                trace!("Handling `{}`", $method);
                $(
                    if $method == <$n_action as Action>::METHOD {
                        let notification = msg.parse_as_notification::<$n_action>()?;
                        if let Err(_) = notification.dispatch(&mut self.state, &mut self.ctx, self.output.clone()) {
                            debug!("Error handling notifcation: {:?}", msg);
                        }
                        handled = true;
                    }
                )*
                $(
                    if $method == <$r_action as Action>::METHOD {
                        let request = msg.parse_as_request::<$r_action>()?;
                        if let Err(_) = request.dispatch(&mut self.state, &mut self.ctx, self.output.clone()) {
                            debug!("Error handling notifcation: {:?}", msg);
                        }
                        handled = true;
                    }
                )*
                if !handled {
                    debug!("Method not found: {}", $method);
                }
            }
        }

        match_action!(
            msg.method;
            notifications:
                ExitNotification,
                notifications::Initialized,
                notifications::DidOpen,
                notifications::DidChange,
                notifications::Cancel,
                notifications::DidSave,
                notifications::DidChangeWatchedFiles;
            requests:
                ShutdownRequest,
                InitializeRequest,
                requests::Completion,
                requests::ResolveCompletion;
        );

        Ok(())
    }

    pub fn handle_message(&mut self) -> ServerStateChange {
        let msg_string = match self.msg_reader.read_message() {
            Some(m) => m,
            None => {
                debug!("Can't read message");
                self.output.failure(Id::Null, jsonrpc::Error::parse_error());
                return ServerStateChange::Break;
            }
        };

        trace!("Read message `{}`", msg_string);

        {
            let shut_down = self.state.shut_down.load(Ordering::SeqCst);
            if shut_down {
                if msg_string != ExitNotification::METHOD {
                    // We're shutdown, ignore any messages other than 'exit'. This is not actually
                    // in the spec, I'm not sure we should do this, but it kinda makes sense.
                    return ServerStateChange::Continue;
                }
            }
        }

        let raw_message = match self.parse_message(&msg_string) {
            Ok(Some(rm)) => rm,
            Ok(None) => return ServerStateChange::Continue,
            Err(e) => {
                debug!("parsing error, {:?}", e);
                self.output.failure(Id::Null, jsonrpc::Error::parse_error());
                return ServerStateChange::Break;
            }
        };

        trace!("Parsed message `{:?}`", raw_message);

        if let Err(e) = self.dispatch_message(&raw_message) {
            debug!("dispatch error, {:?}", e);
            self.output.failure(raw_message.id.unwrap_or(Id::Null), e);
            return ServerStateChange::Break;
        }

        ServerStateChange::Continue
    }
}

#[derive(Debug)]
struct RawMessage {
    method: String,
    id: Option<Id>,
    params: json::Value,
}

impl RawMessage {
    fn parse_as_request<'a, T: RequestAction<'a>>(&'a self) -> Result<Request<T>, jsonrpc::Error> {

        // FIXME: We only support numeric responses, ideally we should switch from using parsed usize
        // to using jsonrpc_core::Id
        let parsed_numeric_id = match &self.id {
            &Some(Id::Num(n)) => Some(n as usize),
            &Some(Id::Str(ref s)) => usize::from_str_radix(s, 10).ok(),
            _ => None,
        };

        let params = T::Params::deserialize(&self.params).map_err(|e| {
            debug!("error when parsing as request: {}", e);
            jsonrpc::Error::invalid_request()
        })?;

        match parsed_numeric_id {
            Some(id) => {
                Ok(Request {
                    id,
                    params,
                    _action: PhantomData,
                })
            }
            None => return Err(jsonrpc::Error::invalid_request()),
        }
    }

    fn parse_as_notification<'a, T: NotificationAction<'a>>(
        &'a self,
    ) -> Result<Notification<T>, jsonrpc::Error> {
        use serde::Deserialize;

        let params = T::Params::deserialize(&self.params).map_err(|e| {
            debug!("error when parsing as notification: {}", e);
            jsonrpc::Error::invalid_request()
        })?;

        Ok(Notification {
            params,
            _action: PhantomData,
        })
    }
}
