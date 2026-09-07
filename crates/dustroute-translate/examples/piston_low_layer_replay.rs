//! Diagnostic replay of the same bounded snapshots/lever edges used by the
//! live low-layer probe. Does not grant placement or MCP execution capability.
use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{
    BlockKind, PistonState, PistonVariant, Pos, Region, World, piston_state, piston_variant,
};
use dustroute_translate::{MinecraftSnapshot, world_from_snapshot};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Case {
    schema_version: String,
    id: String,
    minecraft_version: String,
    initial: MinecraftSnapshot,
    input: Option<Pos>,
    actions: Vec<Action>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Action {
    Toggle(bool),
    At { input: Pos, powered: bool },
}

fn observation(world: &World, bounds: &MinecraftSnapshot) -> Value {
    let mut records = Vec::new();
    for x in bounds.min.x..=bounds.max.x {
        for y in bounds.min.y..=bounds.max.y {
            for z in bounds.min.z..=bounds.max.z {
                let pos = Pos::new(x, y, z);
                let block = world.get(pos);
                let mut properties = serde_json::Map::new();
                let name = match block.map(|b| b.kind) {
                    None | Some(BlockKind::Air) => "minecraft:air".to_owned(),
                    Some(BlockKind::Piston) => {
                        let b = block.unwrap();
                        properties.insert(
                            "facing".into(),
                            json!(format!("{:?}", b.facing.unwrap()).to_lowercase()),
                        );
                        properties.insert(
                            "extended".into(),
                            json!(piston_state(b) == PistonState::Extended),
                        );
                        if piston_variant(b) == PistonVariant::Sticky {
                            "minecraft:sticky_piston"
                        } else {
                            "minecraft:piston"
                        }
                        .into()
                    }
                    Some(BlockKind::PistonHead) => {
                        let b = block.unwrap();
                        properties.insert(
                            "facing".into(),
                            json!(format!("{:?}", b.facing.unwrap()).to_lowercase()),
                        );
                        properties.insert(
                            "type".into(),
                            json!(if piston_variant(b) == PistonVariant::Sticky {
                                "sticky"
                            } else {
                                "normal"
                            }),
                        );
                        "minecraft:piston_head".into()
                    }
                    Some(BlockKind::RedstoneWire) => {
                        properties.insert(
                            "power".into(),
                            json!(block.unwrap().power_level.unwrap_or(0)),
                        );
                        "minecraft:redstone_wire".into()
                    }
                    Some(BlockKind::Lever) => {
                        let b = block.unwrap();
                        properties.insert("powered".into(), json!(b.powered.unwrap()));
                        properties.insert("facing".into(), json!(b.observed_properties["facing"]));
                        "minecraft:lever".into()
                    }
                    Some(_) => block
                        .unwrap()
                        .observed_name
                        .clone()
                        .unwrap_or_else(|| "minecraft:stone".into()),
                };
                records.push(json!({ "pos": pos, "name": name, "properties": properties }));
            }
        }
    }
    json!(records)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected case JSON path")?;
    let case: Case = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    if case.schema_version != "dustroute.piston-low-layer-case.v1"
        || case.minecraft_version != "1.21.11"
    {
        return Err("unsupported diagnostic contract".into());
    }
    let world = world_from_snapshot(&case.initial)?;
    let placement_issues = world.placement_issues();
    // The engine budget is cumulative across all input edges, not per run.
    let event_budget = case
        .actions
        .len()
        .max(1)
        .checked_mul(512)
        .ok_or("event budget overflow")?;
    let mut engine = PhysicsEngine::new_diagnostic(world, event_budget)
        .with_piston_planning_region(Region::new(case.initial.min, case.initial.max));
    let initial = observation(engine.world(), &case.initial);
    let mut phases = Vec::new();
    for action in case.actions {
        let (input, powered) = match action {
            Action::Toggle(powered) => (case.input.ok_or("missing default input")?, powered),
            Action::At { input, powered } => (input, powered),
        };
        engine.schedule_redstone_input(engine.time().game_tick + 1, input, powered);
        let processed_before = engine.processed_events();
        let result = engine.run_redstone_propagation();
        phases.push(json!({ "powered": powered, "error": result.as_ref().err().map(ToString::to_string),
            "state": observation(engine.world(), &case.initial), "trace_status": engine.trace_status(),
            "pending_events": engine.pending_event_count(), "processed_events": engine.processed_events(), "phase_events": engine.processed_events() - processed_before }));
        if result.is_err() {
            break;
        }
    }
    println!(
        "{}",
        json!({ "case_id": case.id, "execution_mode": "diagnostic", "placement_issues": placement_issues,
        "event_budget": event_budget, "initial": initial, "phases": phases, "event_trace": engine.event_trace(), "transition_trace": engine.transition_trace() })
    );
    Ok(())
}
