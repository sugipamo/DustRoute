use rmcp::{ServiceExt, transport::stdio};

use dustroute_mcp::{DustRouteMcp, McpPolicy};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bridge =
        std::env::var("DUSTROUTE_BOT_BRIDGE").unwrap_or_else(|_| "127.0.0.1:25580".to_owned());
    let policy = McpPolicy::from_environment().map_err(anyhow::Error::msg)?;
    let service = DustRouteMcp::with_policy(bridge, policy)
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
