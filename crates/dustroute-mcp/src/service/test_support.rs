//! Offline MCP harness. Transport fixtures model acknowledgements/readbacks;
//! they never act as the Minecraft physics oracle or start a Minecraft server.
use super::{DustRouteMcp, McpPolicy, PlanStateStore};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientInfo, ContentBlock},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[derive(Default)]
pub(super) struct Fake {
    pub(super) snapshot: Option<Value>,
    pub(super) steps: VecDeque<Value>,
    pub(super) writes: usize,
    pub(super) partial: bool,
    pub(super) unknown_features: bool,
    pub(super) wrong_target: bool,
    pub(super) fail_after_write: Option<usize>,
    pub(super) lose_write_reply_at: Option<usize>,
    pub(super) unverified_readback: bool,
}
pub(super) async fn start_construction_bridge(
    durable_root: PathBuf,
) -> (
    Arc<std::sync::Mutex<Fake>>,
    String,
    tokio::task::JoinHandle<()>,
) {
    let fake = Arc::new(std::sync::Mutex::new(Fake::default()));
    let transport = fake.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let bridge = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let result = {
                let mut state = transport.lock().unwrap();
                match req["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"bot","host":if state.wrong_target {"other-server"}else{"localhost"},"port":25565,"version":"1.21.11","dimension":"minecraft:overworld","enabled_features":if state.unknown_features {Value::Null}else{json!(["minecraft:vanilla"])}})
                    }
                    "observe_player" => {
                        json!({"player":"Tester","eye_position":{"x":0.0,"y":180.0,"z":0.0},"yaw":0.0,"pitch":0.0,"dimension":"minecraft:overworld"})
                    }
                    "scan_region" => {
                        let mut snapshot = state.snapshot.clone().unwrap_or_else(|| json!({"min":req["params"]["min"],"max":req["params"]["max"],"blocks":[]}));
                        if state.partial
                            || state.fail_after_write.is_some_and(|n| state.writes >= n)
                        {
                            snapshot["min"]["x"] =
                                json!(snapshot["min"]["x"].as_i64().unwrap() + 1);
                        }
                        snapshot
                    }
                    "preview_region" => json!({"particle_corners":8}),
                    "wait_ticks" => json!({"waited":true}),
                    "submit_command_batch" => {
                        // Transport stub only: physical callback conformance is
                        // covered by independent server captures, not this mock.
                        let expected = state.steps.pop_front().expect("unexpected write");
                        let active: Vec<Value> = fs::read_dir(&durable_root)
                            .unwrap()
                            .filter_map(|entry| {
                                let path = entry.unwrap().path();
                                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                                    return None;
                                }
                                let record: Value =
                                    serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                                (record["state"] == "needs_inspection").then_some(record)
                            })
                            .collect();
                        assert_eq!(
                            active.len(),
                            1,
                            "durable intent must exist before every write"
                        );
                        let attempt = active[0]["attempts"].as_array().unwrap().last().unwrap();
                        assert_eq!(
                            attempt["verified_steps"].as_u64().unwrap() as usize,
                            attempt["total_steps"].as_u64().unwrap() as usize
                                - state.steps.len()
                                - 1
                        );
                        assert_eq!(
                            req["params"]["changes"],
                            json!([{"pos":expected["position"],"state":expected["state"]}])
                        );
                        state.snapshot = Some(expected["expected"].clone());
                        state.writes += 1;
                        json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":1})
                    }
                    method => panic!("unexpected transport method {method}"),
                }
            };
            if req["method"] == "submit_command_batch" {
                let state = transport.lock().unwrap();
                if state.lose_write_reply_at == Some(state.writes) {
                    // The simulated server applied the command but the TCP
                    // reply is lost. No process or host fault is injected.
                    continue;
                }
            }
            let result = if transport.lock().unwrap().unverified_readback {
                result
            } else {
                crate::bridge::test_readback_response(&req, result)
            };
            stream
                .write_all(format!("{}\n", json!({"id":req["id"],"result":result})).as_bytes())
                .await
                .unwrap();
        }
    });
    (fake, address, bridge)
}

pub(super) async fn connected(root: &Path, address: &str) -> (Client, tokio::task::JoinHandle<()>) {
    let mut service = DustRouteMcp::with_policy_and_player(
        address,
        McpPolicy {
            read_only: false,
            ..Default::default()
        },
        "Tester",
    );
    service.state_store = PlanStateStore::new(root.to_path_buf(), 3600);
    serve(service).await
}

pub(super) type Client = rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>;
pub(super) async fn start(root: &Path) -> (Client, tokio::task::JoinHandle<()>) {
    // A dead bridge endpoint proves these are local catalog operations, even with read-only world policy.
    let mut service =
        DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.to_path_buf(), 3600);
    serve(service).await
}
pub(super) async fn call(client: &Client, name: &str, args: Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let ContentBlock::Text(text) = &result.content[0] else {
        panic!("expected JSON text")
    };
    serde_json::from_str(&text.text)
        .unwrap_or_else(|e| panic!("{name} returned non-JSON: {} ({e})", text.text))
}
pub(super) fn temporary() -> PathBuf {
    std::env::temp_dir().join(format!("dustroute-mcp-blueprints-{}", uuid::Uuid::new_v4()))
}
pub(super) async fn stop(client: Client, server: tokio::task::JoinHandle<()>) {
    client.cancel().await.unwrap();
    server.await.unwrap();
}

async fn serve(service: DustRouteMcp) -> (Client, tokio::task::JoinHandle<()>) {
    let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
    let server = tokio::spawn(async move {
        service
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    (
        ClientInfo::default().serve(client_io).await.unwrap(),
        server,
    )
}
