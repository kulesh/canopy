use std::path::PathBuf;

use canopy_lib::{run, AppConfig, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::new(
        PathBuf::from("."),
        None,
        "example".to_string(),
        "Example architecture walkthrough".to_string(),
        false,
    );
    run(config).await
}
