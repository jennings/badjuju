use clap::{Parser, Subcommand};
use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

use badjuju::commands;
use badjuju::jj::Jj;
use badjuju::server::Backend;
use badjuju::workspace::find_workspace_root;

#[derive(Parser)]
#[command(name = "badjuju", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the LSP server over stdio
    Lsp,
    /// Write status.jujutsu and print its absolute path (e.g. hx "$(badjuju status)")
    Status,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Lsp => {
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            let (service, socket) = LspService::new(Backend::new);
            Server::new(stdin, stdout, socket).serve(service).await;
        }
        Command::Status => {
            let cwd = match std::env::current_dir() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("badjuju: cannot determine current directory: {e}");
                    std::process::exit(1);
                }
            };
            let workspace = match find_workspace_root(&cwd) {
                Some(p) => p,
                None => {
                    eprintln!("badjuju: no jj workspace found from {}", cwd.display());
                    std::process::exit(1);
                }
            };
            let jj = Jj::new("jj", &workspace);
            let uri = match commands::run_status(&jj, &workspace) {
                Ok(u) => u,
                Err(e) => {
                    eprintln!("badjuju: {e}");
                    std::process::exit(1);
                }
            };
            let path = match commands::path_from_uri(&uri) {
                Some(p) => p,
                None => {
                    eprintln!("badjuju: could not convert URI to path: {uri}");
                    std::process::exit(1);
                }
            };
            println!("{}", path.display());
        }
    }
}
