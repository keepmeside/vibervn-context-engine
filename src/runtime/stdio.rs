//! Stdio MCP entry point for Claude Code, Codex, Cursor, and similar clients.

use context_engine_rs::engine_boot::{BootOptions, boot_engine_with_home};
use context_engine_rs::mcp::McpHandler;
use rmcp::ServiceExt;

use crate::Cli;

pub async fn run(cli: &Cli) {
    let booted = match boot_engine_with_home(
        BootOptions {
            data_dir: cli.data_dir.clone(),
            embeddings_dir: cli.embeddings_dir.clone(),
            no_watchers: false,
            only_repo: None,
        },
        cli.home_dir.clone(),
    )
    .await
    {
        Ok(booted) => booted,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(2);
        }
    };

    let settings = booted.settings.read().await.clone();
    let handler = McpHandler::new(
        booted.home_dir,
        booted.data_dir,
        booted.index_engine,
        booted.repo_dbs,
        booted.settings,
        &settings.enabled_mcp_tools,
    );

    let transport = rmcp::transport::stdio();
    match handler.serve(transport).await {
        Ok(service) => {
            if let Err(error) = service.waiting().await {
                eprintln!("error: MCP stdio service stopped: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("error: could not start MCP stdio service: {error}");
            std::process::exit(1);
        }
    }
}
