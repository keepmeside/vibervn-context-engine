use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

mod runtime;

#[derive(Parser, Debug)]
#[command(name = "context-engine", about = "Context Engine settings server")]
struct Cli {
    /// Port to listen on [env: CONTEXT_ENGINE_PORT]
    #[arg(long, env = "CONTEXT_ENGINE_PORT")]
    port: Option<u16>,

    /// Bind address [env: CONTEXT_ENGINE_BIND]
    #[arg(long, env = "CONTEXT_ENGINE_BIND")]
    bind: Option<String>,

    /// Data directory base. RocksDB lives below this directory while settings
    /// remain in the settings home.
    #[arg(long, env = "CONTEXT_ENGINE_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Shared content-addressed embedding-cache root.
    #[arg(long, env = "CONTEXT_ENGINE_EMBEDDINGS_DIR")]
    embeddings_dir: Option<PathBuf>,

    /// Internal settings-home override propagated from router to worker.
    #[arg(long, hide = true)]
    home_dir: Option<PathBuf>,

    /// Run as the process-per-project worker for this repository.
    #[arg(long, value_name = "REPO")]
    worker: Option<String>,

    /// Worker idle window before scale-to-zero. Ignored in router mode.
    #[arg(long, env = "CONTEXT_ENGINE_WORKER_IDLE_SECS")]
    worker_idle_secs: Option<u64>,

    /// Run the MCP server over stdin/stdout for subprocess-launched clients.
    /// This mode is mutually exclusive with the process-per-project worker.
    #[arg(long)]
    mcp_stdio: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if cli.mcp_stdio && cli.worker.is_some() {
        eprintln!("error: --mcp-stdio cannot be combined with --worker");
        std::process::exit(2);
    }
    init_tracing(cli.worker.is_some(), cli.mcp_stdio);

    if cli.mcp_stdio {
        runtime::stdio::run(&cli).await;
        return;
    }

    let bind = cli.bind.as_deref().unwrap_or("127.0.0.1").to_owned();
    match cli.worker.clone() {
        Some(repo) => runtime::worker::run(&cli, &bind, repo).await,
        None => runtime::router::run(&cli, &bind).await,
    }
}

fn init_tracing(worker_mode: bool, stdio_mode: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("context_engine_rs=info,warn"));
    if worker_mode || stdio_mode {
        // Worker/MCP stdout is reserved for a machine-readable protocol.
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
