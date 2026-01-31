//! CodeGraph LSP Server Entry Point
//!
//! This is the main entry point for the CodeGraph Language Server.
//! It supports two modes:
//! - LSP mode (default): Serves Language Server Protocol over stdio for editors
//! - MCP mode (--mcp): Serves Model Context Protocol over stdio for AI clients

use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "codegraph-lsp")]
#[command(about = "CodeGraph Language Server with MCP support")]
#[command(version)]
struct Args {
    /// Run in MCP (Model Context Protocol) mode for AI clients
    #[arg(long)]
    mcp: bool,

    /// Run in LSP mode over stdio (default, kept for compatibility)
    #[arg(long)]
    stdio: bool,

    /// Workspace directory to index (required for MCP mode)
    #[arg(long, short)]
    workspace: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging
    let log_filter = if args.mcp {
        // MCP mode: more verbose logging to stderr
        "codegraph_lsp=debug,codegraph=info"
    } else {
        "codegraph_lsp=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| log_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    if args.mcp {
        // MCP mode
        let workspace = args
            .workspace
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        tracing::info!("Starting CodeGraph MCP server");
        tracing::info!("Workspace: {:?}", workspace);

        let mut server = codegraph_lsp::mcp::McpServer::new(workspace);
        if let Err(e) = server.run().await {
            tracing::error!("MCP server error: {}", e);
            std::process::exit(1);
        }
    } else {
        // LSP mode (default)
        use codegraph_lsp::CodeGraphBackend;
        use tower_lsp::{LspService, Server};

        tracing::info!("Starting CodeGraph LSP server");

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(CodeGraphBackend::new);

        Server::new(stdin, stdout, socket).serve(service).await;
    }
}
