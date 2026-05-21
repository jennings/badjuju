use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tracing::{info, warn};

use crate::commands;
use crate::jj::Jj;
use crate::workspace::find_workspace_root;

pub const COMMANDS: &[&str] = &[
    "badjuju.status",
    "badjuju.log",
    "badjuju.describe",
    "badjuju.new",
];

#[derive(Debug, Default)]
struct State {
    workspace_root: Option<PathBuf>,
    binary_path: Option<String>,
}

impl State {
    fn jj(&self) -> Option<Jj> {
        let root = self.workspace_root.as_ref()?;
        Some(Jj::with_binary_or_default(
            self.binary_path.as_deref(),
            root,
        ))
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

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let binary_path = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("binaryPath"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

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
            state.workspace_root = workspace_root.clone();
        }

        if let Some(ref root) = workspace_root {
            info!(root = %root.display(), "found jj workspace");
        } else {
            warn!("no jj workspace found; commands will return errors until a workspace is opened");
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
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

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        info!(command = %params.command, "execute_command");

        let state = self.state.read().await;
        let jj = state.jj().ok_or_else(|| Error::invalid_request())?;

        let workspace = state
            .workspace_root
            .clone()
            .ok_or_else(Error::invalid_request)?;

        let map_err = |e: commands::CommandError| {
            let mut err = Error::internal_error();
            err.message = e.to_string().into();
            err
        };

        match params.command.as_str() {
            "badjuju.status" => {
                let uri = commands::run_status(&jj, &workspace).map_err(map_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.log" => {
                let revset = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("@");
                let uri = commands::run_log(&jj, &workspace, revset).map_err(map_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.describe" => {
                let uri = commands::run_describe(&jj, &workspace).map_err(map_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.new" => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        "command: badjuju.new (not yet implemented)",
                    )
                    .await;
                Ok(None)
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
        assert!(COMMANDS.contains(&"badjuju.new"));
    }
}
