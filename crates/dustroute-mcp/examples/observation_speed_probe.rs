//! Read-only live measurement with a separate dummy player and native MCP.
//! Only player positioning is external; this probe never places or activates blocks.
#[cfg(not(feature = "voxrig"))]
fn main() {
    eprintln!("requires --features voxrig");
}

#[cfg(feature = "voxrig")]
#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    use dustroute_mcp::{DustRouteMcp, McpConfig, McpPolicy};
    use rmcp::transport::{
        StreamableHttpServerConfig,
        streamable_http_server::{
            session::local::LocalSessionManager, tower::StreamableHttpService,
        },
    };
    use std::io::{self, Write};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;
    use voxrig::{Client, ConnectionConfig, MinecraftVersion, Server};

    let port: u16 = std::env::var("MC_PORT")?.parse()?;
    let bind: std::net::SocketAddr = std::env::var("PROBE_HTTP_BIND")?.parse()?;
    anyhow::ensure!(
        bind.ip().is_loopback(),
        "measurement listener must be loopback"
    );
    let dummy_name = std::env::var("PROBE_DUMMY")?;
    let observer_name = std::env::var("PROBE_OBSERVER")?;
    anyhow::ensure!(dummy_name != observer_name, "distinct clients required");
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let started = Instant::now();
    let dummy = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", port),
        &dummy_name,
        MinecraftVersion::Java1_21_11,
    ))
    .await?;
    dummy.wait_until_ready().await?;
    let dummy_ready_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let service = DustRouteMcp::connect_voxrig(
        McpConfig::new(format!("127.0.0.1:{port}"), &dummy_name)?,
        McpPolicy {
            read_only: true,
            allowed_players: [dummy_name.clone()].into(),
            ..Default::default()
        },
        &observer_name,
    )
    .await?;
    let observer_ready_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "READY_FOR_POSITIONING dummy={dummy_name} observer={observer_name} dummy_ready_ms={dummy_ready_ms} observer_ready_ms={observer_ready_ms} http_bind={bind}"
    );
    io::stdout().flush()?;
    // The operator confirms console setup; EOF is an error, never authorization.
    let count = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    anyhow::ensure!(count > 0, "positioning barrier closed");
    let controls = dummy.java_1_21_11_operations()?;
    controls.set_flying(true).await?;
    // Initial chunk streaming is measured separately from steady tool requests.
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("POSITIONED_DUMMY {:?}", controls.player_state().await?);
    let cancellation = CancellationToken::new();
    let http: StreamableHttpService<DustRouteMcp, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(service.clone()),
        Default::default(),
        StreamableHttpServerConfig::default()
            .with_sse_keep_alive(None)
            .with_cancellation_token(cancellation.child_token()),
    );
    let router = axum::Router::new().nest_service("/mcp", http);
    let shutdown = cancellation.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
    });
    println!("MEASUREMENT_READY http://{bind}/mcp; enter to stop after measurement");
    io::stdout().flush()?;
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(10), server).await???;
    dummy.disconnect().await?;
    println!("MEASUREMENT_STOPPED");
    Ok(())
}
