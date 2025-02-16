/*
 * Copyright 2019 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Based on the reference lsp-server example at <https://github.com/rust-analyzer/lsp-server/blob/master/examples/goto_def.rs>.

use crate::{
    eval::Context,
    types::{Message as StarlarkMessage, Severity},
};
use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, LogMessage,
        PublishDiagnostics,
    },
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, LogMessageParams, MessageType, NumberOrString,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};
use serde::de::DeserializeOwned;

struct Backend {
    connection: Connection,
    starlark: Context,
}

fn to_severity(x: Severity) -> DiagnosticSeverity {
    match x {
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Advice => DiagnosticSeverity::Hint,
        Severity::Disabled => DiagnosticSeverity::Information,
    }
}

fn to_diagnostic(x: StarlarkMessage) -> Diagnostic {
    let range = match x.span {
        Some(s) => Range::new(
            Position::new((s.begin.line - 1) as u64, (s.begin.column - 1) as u64),
            Position::new((s.end.line - 1) as u64, (s.end.column - 1) as u64),
        ),
        _ => Range::default(),
    };
    Diagnostic::new(
        range,
        Some(to_severity(x.severity)),
        Some(NumberOrString::String(x.name)),
        None,
        x.description,
        None,
        None,
    )
}

/// The logic implementations of stuff
impl Backend {
    fn server_capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::Full)),
            ..ServerCapabilities::default()
        }
    }

    fn validate(&self, uri: Url, version: Option<i64>, text: String) {
        let diags = self
            .starlark
            .file_with_contents(&uri.to_string(), text)
            .map(to_diagnostic)
            .collect();
        self.publish_diagnostics(uri, diags, version)
    }

    fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.validate(
            params.text_document.uri,
            Some(params.text_document.version),
            params.text_document.text,
        )
    }

    fn did_change(&self, params: DidChangeTextDocumentParams) {
        // We asked for Sync full, so can just grab all the text from params
        let change = params.content_changes.into_iter().next().unwrap();
        self.validate(
            params.text_document.uri,
            params.text_document.version,
            change.text,
        );
    }

    fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.publish_diagnostics(params.text_document.uri, Vec::new(), None)
    }
}

/// The library style pieces
impl Backend {
    fn send_notification(&self, x: Notification) {
        self.connection
            .sender
            .send(Message::Notification(x))
            .unwrap()
    }

    fn log_message(&self, typ: MessageType, message: &str) {
        self.send_notification(new_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.to_owned(),
        }))
    }

    fn publish_diagnostics(&self, uri: Url, diags: Vec<Diagnostic>, version: Option<i64>) {
        self.send_notification(new_notification::<PublishDiagnostics>(
            PublishDiagnosticsParams::new(uri, diags, version),
        ));
    }

    fn main_loop(&self, _params: InitializeParams) -> anyhow::Result<()> {
        self.log_message(MessageType::Info, "Starlark server initialised");
        for msg in &self.connection.receiver {
            match msg {
                Message::Request(req) => {
                    if self.connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    // Currently don't handle any other requests
                }
                Message::Notification(x) => {
                    if let Some(params) = as_notification::<DidOpenTextDocument>(&x) {
                        self.did_open(params)
                    } else if let Some(params) = as_notification::<DidChangeTextDocument>(&x) {
                        self.did_change(params)
                    } else if let Some(params) = as_notification::<DidCloseTextDocument>(&x) {
                        self.did_close(params)
                    }
                }
                Message::Response(_) => {
                    // Don't expect any of these
                }
            }
        }
        Ok(())
    }
}

pub fn server(starlark: Context) -> anyhow::Result<()> {
    // Note that  we must have our logging only write out to stderr.
    eprintln!("Starting Rust Starlark server");

    let (connection, io_threads) = Connection::stdio();
    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let server_capabilities = serde_json::to_value(&Backend::server_capabilities()).unwrap();
    let initialization_params = connection.initialize(server_capabilities)?;
    let initialization_params = serde_json::from_value(initialization_params).unwrap();
    Backend {
        connection,
        starlark,
    }
    .main_loop(initialization_params)?;
    io_threads.join()?;

    eprintln!("Stopping Rust Starlark server");
    Ok(())
}

fn as_notification<T>(x: &Notification) -> Option<T::Params>
where
    T: lsp_types::notification::Notification,
    T::Params: DeserializeOwned,
{
    if x.method == T::METHOD {
        let params = serde_json::from_value(x.params.clone()).unwrap_or_else(|err| {
            panic!(
                "Invalid notification\nMethod: {}\n error: {}",
                x.method, err
            )
        });
        Some(params)
    } else {
        None
    }
}

fn new_notification<T>(params: T::Params) -> Notification
where
    T: lsp_types::notification::Notification,
{
    Notification {
        method: T::METHOD.to_owned(),
        params: serde_json::to_value(&params).unwrap(),
    }
}
