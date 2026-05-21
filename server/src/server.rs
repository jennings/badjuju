use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tracing::info;

pub const COMMANDS: &[&str] = &[
    "badjuju.status",
    "badjuju.log",
    "badjuju.describe",
    "badjuju.new",
];

#[derive(Debug)]
pub struct Backend {
    client: Client,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
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
        match params.command.as_str() {
            "badjuju.status" | "badjuju.log" | "badjuju.describe" | "badjuju.new" => {
                self.client
                    .log_message(MessageType::INFO, format!("command: {}", params.command))
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
