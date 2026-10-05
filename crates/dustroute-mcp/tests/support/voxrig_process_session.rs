//! Isolated trial transport, never used by the MCP product runtime.
use rmcp::{
    ServiceExt,
    model::ClientInfo,
    service::{RoleClient, RunningService},
};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    sync::watch,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

pub type McpClient = RunningService<RoleClient, ClientInfo>;
pub type Session = (McpClient, ServerTask);
pub enum ServerTask {
    Memory(JoinHandle<anyhow::Result<()>>),
    Process(Child),
}
impl ServerTask {
    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Memory(_) => None,
            Self::Process(child) => child.id(),
        }
    }
}
pub async fn start_process(port: u16) -> anyhow::Result<Session> {
    let binary = PathBuf::from(std::env::var("MCP_PROCESS_BIN")?).canonicalize()?;
    let state = PathBuf::from(std::env::var("DUSTROUTE_STATE_DIR")?);
    let state = if state.is_absolute() {
        state
    } else {
        std::env::current_dir()?.join(state)
    };
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{}.mcp-stderr.log", std::env::var("TRACE_OUTPUT")?))?;
    let mut child = Command::new(binary)
        .env_clear()
        .env("TOKIO_WORKER_THREADS", "2")
        .env("DUSTROUTE_BOT_BACKEND", "voxrig")
        .env("DUSTROUTE_MC_AUTH", "offline")
        .env("DUSTROUTE_MC_VERSION", "1.21.11")
        .env("DUSTROUTE_BOT_NAME", "DustRouteBot")
        .env("DUSTROUTE_ASSIST_PLAYER", "McpProbe")
        .env("DUSTROUTE_SERVER_ADDRESS", format!("127.0.0.1:{port}"))
        .env("DUSTROUTE_MCP_TRANSPORT", "stdio")
        .env("DUSTROUTE_STATE_DIR", state)
        .env("DUSTROUTE_READ_ONLY", "false")
        .env("DUSTROUTE_PREVIEW_REQUIRED", "true")
        .env("DUSTROUTE_ALLOWED_PLAYERS", "McpProbe")
        .env("DUSTROUTE_ALLOWED_DIMENSIONS", "minecraft:overworld")
        .env("DUSTROUTE_ALLOWED_REGION", "80,170,80,120,205,120")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()?;
    let input = child.stdin.take().unwrap();
    let output = child.stdout.take().unwrap();
    let client = tokio::time::timeout(
        Duration::from_secs(30),
        ClientInfo::default().serve((output, input)),
    )
    .await??;
    Ok((client, ServerTask::Process(child)))
}
pub async fn stop((client, server): Session) -> anyhow::Result<()> {
    client.cancel().await?;
    match server {
        ServerTask::Memory(task) => {
            task.await??;
        }
        ServerTask::Process(mut child) => {
            let status = tokio::time::timeout(Duration::from_secs(15), child.wait()).await??;
            anyhow::ensure!(status.success(), "MCP process exit: {status}");
        }
    }
    Ok(())
}

/// One owned loopback listener keeps the recorded server endpoint stable across
/// child restarts. A requested cut closes only its current Minecraft TCP streams.
pub struct Proxy {
    pub port: u16,
    cut: watch::Sender<u64>,
    stop: CancellationToken,
    task: JoinHandle<anyhow::Result<()>>,
}
impl Proxy {
    pub async fn start(server_port: u16) -> anyhow::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let (cut, receiver) = watch::channel(0u64);
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            let mut streams = JoinSet::new();
            loop {
                tokio::select! {
                    _=shutdown.cancelled() => break,
                    Some(result)=streams.join_next(), if !streams.is_empty() => { result??; }
                    accepted=listener.accept() => {
                        let (mut incoming,_)=accepted?;
                        let mut upstream=TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,server_port)).await?;
                        let mut change=receiver.clone();
                        change.borrow_and_update();
                        let finish=shutdown.clone();
                        streams.spawn(async move {
                            tokio::select! {
                                _=finish.cancelled()=>{},
                                _=change.changed()=>{},
                                result=tokio::io::copy_bidirectional(&mut incoming,&mut upstream)=>{
                                    // Normal remote disconnect can include a TCP reset.
                                    if let Err(e)=result && e.kind()!=std::io::ErrorKind::ConnectionReset {
                                        return Err(anyhow::Error::from(e));
                                    }
                                }
                            }
                            Ok(())
                        });
                    }
                }
            }
            while let Some(result) = streams.join_next().await {
                result??;
            }
            Ok(())
        });
        Ok(Self {
            port,
            cut,
            stop,
            task,
        })
    }
    pub fn cut_connection(&self) {
        self.cut.send_modify(|generation| *generation += 1);
    }
    pub async fn stop(self) -> anyhow::Result<()> {
        self.stop.cancel();
        self.task.await??;
        Ok(())
    }
}
