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
    /// Write log.jujutsu and print its absolute path
    Log {
        /// Revset to show (defaults to the mutable-ancestors view)
        #[arg(long)]
        revset: Option<String>,
    },
    /// Write diff.jujutsu and print its absolute path
    Diff {
        /// Revision to diff (defaults to @)
        #[arg(long)]
        revision: Option<String>,
    },
}

fn resolve_workspace() -> (Jj, std::path::PathBuf) {
    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("badjuju: cannot determine current directory: {e}");
        std::process::exit(1);
    });
    let workspace = find_workspace_root(&cwd).unwrap_or_else(|| {
        eprintln!("badjuju: no jj workspace found from {}", cwd.display());
        std::process::exit(1);
    });
    let jj = Jj::new("jj", &workspace);
    (jj, workspace)
}

fn print_path(uri: &str) {
    match commands::path_from_uri(uri) {
        Some(p) => println!("{}", p.display()),
        None => {
            eprintln!("badjuju: could not convert URI to path: {uri}");
            std::process::exit(1);
        }
    }
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
            let (jj, workspace) = resolve_workspace();
            let uri = commands::run_status(&jj, &workspace).unwrap_or_else(|e| {
                eprintln!("badjuju: {e}");
                std::process::exit(1);
            });
            print_path(&uri);
        }
        Command::Log { revset } => {
            let (jj, workspace) = resolve_workspace();
            let uri =
                commands::run_log(&jj, &workspace, revset.as_deref().unwrap_or(""))
                    .unwrap_or_else(|e| {
                        eprintln!("badjuju: {e}");
                        std::process::exit(1);
                    });
            print_path(&uri);
        }
        Command::Diff { revision } => {
            let (jj, workspace) = resolve_workspace();
            let uri =
                commands::run_diff(&jj, &workspace, revision.as_deref().unwrap_or("@"))
                    .unwrap_or_else(|e| {
                        eprintln!("badjuju: {e}");
                        std::process::exit(1);
                    });
            print_path(&uri);
        }
    }
}
