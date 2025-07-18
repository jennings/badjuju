use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use watchman_client::Connector;

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

// #[tokio::main]
fn main() {
    // let stdin = tokio::io::stdin();
    // let stdout = tokio::io::stdout();

    let status_output = Command::new("jj").args(&["status"]).output().unwrap();

    println!("{}", String::from_utf8(status_output.stdout).unwrap());
}
