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
    let handler = match std::env::var("DUSTROUTE_BOT_BACKEND")
        .unwrap_or_else(|_| {
            if cfg!(feature = "voxrig") {
                "voxrig"
            } else {
                "mineflayer"
            }
            .into()
        })
        .as_str()
    {
        "mineflayer" => DustRouteMcp::with_config(config, policy),
        #[cfg(feature = "voxrig")]
        "voxrig" => {
            anyhow::ensure!(
                std::env::var("DUSTROUTE_MC_AUTH").unwrap_or_else(|_| "offline".into())
                    == "offline",
                "Voxrig currently requires offline authentication"
            );
            anyhow::ensure!(
                std::env::var("DUSTROUTE_MC_VERSION").unwrap_or_else(|_| "1.21.11".into())
                    == "1.21.11",
                "DustRoute native client requires Java 1.21.11"
            );
            let username =
                std::env::var("DUSTROUTE_BOT_NAME").unwrap_or_else(|_| "DustRouteBot".into());
            DustRouteMcp::connect_voxrig(config, policy, &username).await?
        }
        #[cfg(not(feature = "voxrig"))]
        "voxrig" => anyhow::bail!("Voxrig backend requires compilation with --features voxrig"),
        value => {
            anyhow::bail!("unknown DUSTROUTE_BOT_BACKEND {value:?}; expected mineflayer or voxrig")
        }
    };
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
