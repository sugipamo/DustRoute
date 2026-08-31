use rmcp::{
    ServiceExt,
    transport::{
        StreamableHttpServerConfig, stdio,
        streamable_http_server::{
            session::local::LocalSessionManager, tower::StreamableHttpService,
        },
    },
};
use tokio_util::sync::CancellationToken;

use dustroute_mcp::{DustRouteMcp, McpConfig, McpPolicy, McpTransport};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = McpConfig::from_environment()?;
    let policy = McpPolicy::from_environment().map_err(anyhow::Error::msg)?;
    let transport = McpTransport::from_environment().map_err(anyhow::Error::msg)?;
    let handler = DustRouteMcp::with_config(config, policy);
    match transport {
        McpTransport::Stdio => {
            handler.serve(stdio()).await?.waiting().await?;
        }
        McpTransport::Http(address) => serve_http(handler, address).await?,
    }
    Ok(())
}

async fn serve_http(handler: DustRouteMcp, address: std::net::SocketAddr) -> anyhow::Result<()> {
    let cancellation = CancellationToken::new();
    let service: StreamableHttpService<DustRouteMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(handler.clone()),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_sse_keep_alive(None)
                .with_cancellation_token(cancellation.child_token()),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("DustRoute MCP listening on http://{address}/mcp (loopback only)");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}
