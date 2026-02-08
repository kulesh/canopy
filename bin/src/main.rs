use std::path::PathBuf;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use canopy_lib::{run, AppConfig, Result};

#[derive(Debug, Parser)]
#[command(name = "canopy", about = "Canopy: flight deck for agentic coding")]
struct Cli {
    #[arg(value_name = "PATH", default_value = ".")]
    path: PathBuf,

    #[arg(long, value_name = "WORKSPACE_TOML")]
    workspace: Option<PathBuf>,

    #[arg(long, env = "CANOPY_AUTHOR", default_value = "unknown")]
    author: String,

    #[arg(
        long,
        env = "CANOPY_PURPOSE",
        default_value = "Understand the repository architecture for safe agentic changes"
    )]
    purpose: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    let config = AppConfig::new(cli.path, cli.workspace, cli.author, cli.purpose);
    run(config).await
}
