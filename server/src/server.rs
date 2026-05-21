use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tracing::{info, warn};

use crate::commands::{self, CommandReference};
use crate::jj::Jj;
use crate::workspace::find_workspace_root;

pub const COMMANDS: &[&str] = &[
    "badjuju.status",
    "badjuju.log",
    "badjuju.describe",
    "badjuju.diff",
    "badjuju.new",
    "badjuju.next",
    "badjuju.prev",
    "badjuju.refresh",
    "badjuju.squash",
    "badjuju.unsquash",
    "badjuju.toggleStat",
    "badjuju.undo",
    "badjuju.abandon",
];

#[derive(Debug, Default)]
struct State {
    workspace_root: Option<PathBuf>,
    binary_path: Option<String>,
    /// Client-supplied COMMAND REFERENCE overrides (one entry per generated
    /// buffer). Captured at `initialize` time and attached to every `Jj`
    /// instance the server creates.
    command_reference: CommandReference,
    /// Latest text content for open documents, keyed by URI string.
    documents: HashMap<String, String>,
}

impl State {
    fn jj(&self) -> Option<Jj> {
        let root = self.workspace_root.as_ref()?;
        Some(
            Jj::with_binary_or_default(self.binary_path.as_deref(), root)
                .with_command_reference(self.command_reference.clone()),
        )
    }
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    state: Arc<RwLock<State>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(State::default())),
        }
    }
}

fn lsp_err(msg: impl ToString) -> Error {
    let mut err = Error::internal_error();
    err.message = msg.to_string().into();
    err
}

/// Parse the optional `commandReference` object passed in `initializationOptions`.
/// Each of `status`, `log`, and `diff` is an optional string. Missing or
/// non-string values fall back to the server's built-in defaults.
fn parse_command_reference(value: &serde_json::Value) -> CommandReference {
    let pick = |key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    CommandReference::new(pick("status"), pick("log"), pick("diff"))
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let binary_path = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("binaryPath"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let command_reference = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("commandReference"))
            .map(parse_command_reference)
            .unwrap_or_default();

        let search_start = params
            .root_uri
            .as_ref()
            .and_then(|u| u.to_file_path().ok())
            .or_else(|| {
                params
                    .workspace_folders
                    .as_deref()
                    .and_then(|f| f.first())
                    .and_then(|f| f.uri.to_file_path().ok())
            });

        let workspace_root = search_start.as_deref().and_then(find_workspace_root);

        {
            let mut state = self.state.write().await;
            state.binary_path = binary_path;
            state.command_reference = command_reference;
            state.workspace_root = workspace_root.clone();
        }

        if let Some(ref root) = workspace_root {
            info!(root = %root.display(), "found jj workspace");
        } else {
            warn!("no jj workspace found; commands will return errors until a workspace is opened");
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: COMMANDS.iter().map(|s| s.to_string()).collect(),
                    work_done_progress_options: Default::default(),
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("server initialized");
        self.client
            .log_message(MessageType::INFO, "badjuju server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text;
        self.state.write().await.documents.insert(uri, text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().next() {
            self.state.write().await.documents.insert(uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.state.write().await.documents.remove(&uri);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri_str = params.text_document.uri.to_string();

        let state = self.state.read().await;
        let (jj, workspace) = match (state.jj(), state.workspace_root.clone()) {
            (Some(jj), Some(root)) => (jj, root),
            _ => {
                self.client
                    .log_message(MessageType::WARNING, "did_save: no jj workspace")
                    .await;
                return;
            }
        };

        // Get content from params (if include_text worked) or fall back to cached doc.
        let content = params
            .text
            .or_else(|| state.documents.get(&uri_str).cloned());

        let Some(text) = content else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("did_save: no content for {uri_str}"),
                )
                .await;
            return;
        };

        let filename = params
            .text_document
            .uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));

        drop(state);

        match filename.as_deref() {
            Some("describe.jujutsu") => {
                if let Err(e) = commands::on_describe_save(&jj, &workspace, &text) {
                    self.client
                        .log_message(MessageType::ERROR, format!("describe save failed: {e}"))
                        .await;
                }
            }
            Some("log.jujutsu") => {
                if let Err(e) = commands::on_log_save(&jj, &workspace, &text) {
                    self.client
                        .log_message(MessageType::ERROR, format!("log save failed: {e}"))
                        .await;
                }
            }
            _ => {}
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        info!(command = %params.command, "execute_command");

        let state = self.state.read().await;
        let jj = state.jj().ok_or_else(Error::invalid_request)?;
        let workspace = state
            .workspace_root
            .clone()
            .ok_or_else(Error::invalid_request)?;
        drop(state);

        match params.command.as_str() {
            "badjuju.status" => {
                let uri = commands::run_status(&jj, &workspace).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.log" => {
                let revset = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_log(&jj, &workspace, revset).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.describe" => {
                let revision = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_describe(&jj, &workspace, revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.diff" => {
                let revision = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_diff(&jj, &workspace, revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.new" => {
                let parent = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_new(&jj, &workspace, parent).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.next" => {
                let edit = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let uri = commands::run_next(&jj, &workspace, edit).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.prev" => {
                let edit = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let uri = commands::run_prev(&jj, &workspace, edit).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.refresh" => {
                let doc_uri = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_refresh(&jj, &workspace, doc_uri).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.squash" => {
                let file = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let revision = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_squash(&jj, &workspace, file, revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.unsquash" => {
                let file = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let revision = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri =
                    commands::run_unsquash(&jj, &workspace, file, revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.toggleStat" => {
                let uri = commands::run_toggle_stat(&jj, &workspace).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.undo" => {
                let uri = commands::run_undo(&jj, &workspace).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.abandon" => {
                let revision = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_abandon(&jj, &workspace, revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            _ => Err(Error::method_not_found()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_list_is_nonempty() {
        assert!(!COMMANDS.is_empty());
    }

    #[test]
    fn commands_include_expected() {
        assert!(COMMANDS.contains(&"badjuju.status"));
        assert!(COMMANDS.contains(&"badjuju.log"));
        assert!(COMMANDS.contains(&"badjuju.describe"));
        assert!(COMMANDS.contains(&"badjuju.diff"));
        assert!(COMMANDS.contains(&"badjuju.new"));
        assert!(COMMANDS.contains(&"badjuju.next"));
        assert!(COMMANDS.contains(&"badjuju.prev"));
        assert!(COMMANDS.contains(&"badjuju.refresh"));
        assert!(COMMANDS.contains(&"badjuju.squash"));
        assert!(COMMANDS.contains(&"badjuju.unsquash"));
        assert!(COMMANDS.contains(&"badjuju.toggleStat"));
        assert!(COMMANDS.contains(&"badjuju.undo"));
        assert!(COMMANDS.contains(&"badjuju.abandon"));
    }
}
