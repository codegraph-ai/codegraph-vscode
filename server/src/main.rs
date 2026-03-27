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

    /// Workspace directories to index (can be specified multiple times for multi-project)
    #[arg(long, short)]
    workspace: Vec<PathBuf>,

    /// Directories to exclude from indexing (can be specified multiple times)
    #[arg(long, short)]
    exclude: Vec<String>,

    /// Maximum number of files to index (default: 5000)
    #[arg(long, default_value = "5000")]
    max_files: usize,

    /// Embedding model: jina-code-v2 (768d, best quality) or bge-small (384d, 5x faster)
    #[arg(long, default_value = "jina-code-v2")]
    embedding_model: String,

    /// Embed full function body instead of just name+signature (~3x slower, better quality)
    #[arg(long)]
    full_body_embedding: bool,
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
        let workspaces = if args.workspace.is_empty() {
            vec![std::env::current_dir().expect("Failed to get current directory")]
        } else {
            args.workspace
        };

        let embedding_model = match args.embedding_model.as_str() {
            "bge-small" => codegraph_memory::CodeGraphEmbeddingModel::BgeSmall,
            _ => codegraph_memory::CodeGraphEmbeddingModel::JinaCodeV2,
        };

        tracing::info!("Starting CodeGraph MCP server");
        tracing::info!("Workspaces: {:?}", workspaces);
        tracing::info!("Embedding model: {}", embedding_model.display_name());
        tracing::info!("Full-body embedding: {}", args.full_body_embedding);
        if !args.exclude.is_empty() {
            tracing::info!("Excluding: {:?}", args.exclude);
        }

        let mut server = codegraph_lsp::mcp::McpServer::new(workspaces, args.exclude, args.max_files, embedding_model, args.full_body_embedding);
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
