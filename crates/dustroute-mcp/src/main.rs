use rmcp::{ServiceExt, transport::stdio};

use dustroute_mcp::{DustRouteMcp, McpConfig, McpPolicy};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpConfig::from_environment()?;
    let policy = McpPolicy::from_environment().map_err(anyhow::Error::msg)?;
    let service = DustRouteMcp::with_config(config, policy)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
