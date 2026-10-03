use super::*;
use dustroute_physical::Block;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

fn stone(x: i32, y: i32) -> Value {
    json!({"pos":{"x":x,"y":y,"z":0},"name":"minecraft:stone","properties":{}})
}

fn wire(x: i32, y: i32, east: &str) -> Value {
    json!({"pos":{"x":x,"y":y,"z":0},"name":"minecraft:redstone_wire",
        "properties":{"east":east,"west":"side","north":"none","south":"none","power":"0"}})
}

fn place_wire(pos: Pos) -> BlockChange {
    let mut after = Block::new(BlockKind::RedstoneWire);
    after.support_offset = Some(Pos::new(0, -1, 0));
    BlockChange {
        pos,
        before: Block::new(BlockKind::Air),
        after,
        collision: false,
    }
}

async fn validate(
    blocks: Vec<Value>,
    changes: Vec<BlockChange>,
    policy: McpPolicy,
) -> (
    Result<dustroute_app::ValidatedBlockChanges, FailureCause>,
    Vec<RegionBounds>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bridge = BotBridge::with_test_transport(listener.local_addr().unwrap().to_string());
    let scans = Arc::new(Mutex::new(Vec::new()));
    let observed = scans.clone();
    let transport = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(
                request["method"], "scan_region",
                "validation must not write"
            );
            let bounds = RegionBounds::new(
                serde_json::from_value(request["params"]["min"].clone()).unwrap(),
                serde_json::from_value(request["params"]["max"].clone()).unwrap(),
            );
            observed.lock().unwrap().push(bounds);
            let snapshot = crate::bridge::test_scan_world(
                &request,
                json!({"min":bounds.min,"max":bounds.max,"blocks":blocks}),
            );
            let response = json!({"id":request["id"],
                "result":crate::bridge::test_readback_response(&request,snapshot)});
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let result = WorldEditor {
        bridge: &bridge,
        policy: &policy,
    }
    .validate_live_changes(&changes, "minecraft:overworld")
    .await;
    transport.abort();
    let scans = scans.lock().unwrap().clone();
    (result, scans)
}

fn upward_step() -> Vec<Value> {
    vec![
        stone(0, 0),
        wire(0, 1, "side"),
        stone(1, 1),
        stone(2, 1),
        wire(2, 2, "side"),
    ]
}

#[tokio::test]
async fn upward_repair_reads_the_support_below_its_lower_neighbor() {
    let (result, scans) = validate(
        upward_step(),
        vec![place_wire(Pos::new(1, 2, 0))],
        McpPolicy::default(),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(scans.len(), 2);
    assert!(!scans[0].contains(Pos::new(0, 0, 0)));
    assert!(scans[1].contains(Pos::new(0, 0, 0)));
}

#[tokio::test]
async fn observed_missing_support_is_still_rejected() {
    let mut blocks = upward_step();
    blocks.remove(0);
    let (result, scans) = validate(
        blocks,
        vec![place_wire(Pos::new(1, 2, 0))],
        McpPolicy::default(),
    )
    .await;
    assert!(result.unwrap_err().message.contains("InvalidSupport"));
    assert_eq!(scans.len(), 2);
}

#[tokio::test]
async fn additional_context_cannot_exceed_region_or_volume_policy() {
    let initial = RegionBounds::new(Pos::new(0, 1, -1), Pos::new(2, 3, 1));
    for policy in [
        McpPolicy {
            allowed_region: Some(initial.into()),
            ..McpPolicy::default()
        },
        McpPolicy {
            max_scan_volume: 27,
            ..McpPolicy::default()
        },
    ] {
        let (result, scans) =
            validate(upward_step(), vec![place_wire(Pos::new(1, 2, 0))], policy).await;
        assert!(result.is_err());
        assert_eq!(
            scans,
            [initial],
            "policy must reject before requesting more evidence"
        );
    }
}

#[tokio::test]
async fn removing_support_under_an_existing_wire_is_rejected() {
    let change = BlockChange {
        pos: Pos::new(0, 0, 0),
        before: Block::new(BlockKind::Solid),
        after: Block::new(BlockKind::Air),
        collision: false,
    };
    let (result, scans) = validate(
        vec![stone(0, 0), wire(0, 1, "side")],
        vec![change],
        McpPolicy::default(),
    )
    .await;
    assert!(result.unwrap_err().message.contains("InvalidSupport"));
    assert_eq!(scans.len(), 1);
}

#[tokio::test]
async fn explicit_rise_at_scan_edge_requires_observed_upper_wire() {
    let blocks = vec![
        stone(0, 0),
        stone(1, 0),
        wire(1, 1, "up"),
        stone(2, 1),
        wire(2, 2, "side"),
    ];
    let (result, scans) = validate(
        blocks.clone(),
        vec![place_wire(Pos::new(0, 1, 0))],
        McpPolicy::default(),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(scans.len(), 2);
    assert!(scans[1].contains(Pos::new(2, 2, 0)));
    let mut missing_upper = blocks;
    missing_upper.pop();
    let (result, _) = validate(
        missing_upper,
        vec![place_wire(Pos::new(0, 1, 0))],
        McpPolicy::default(),
    )
    .await;
    assert!(
        result
            .unwrap_err()
            .message
            .contains("InvalidWireConnection")
    );
}

#[tokio::test]
async fn an_extending_staircase_stops_at_the_scan_budget() {
    let mut blocks = vec![stone(0, 0)];
    for x in 1..=12 {
        blocks.push(stone(x, x - 1));
        blocks.push(wire(x, x, if x == 12 { "side" } else { "up" }));
    }
    let (result, scans) = validate(
        blocks,
        vec![place_wire(Pos::new(0, 1, 0))],
        McpPolicy::default(),
    )
    .await;
    assert!(result.unwrap_err().message.contains("8-scan limit"));
    assert_eq!(scans.len(), 8);
}
