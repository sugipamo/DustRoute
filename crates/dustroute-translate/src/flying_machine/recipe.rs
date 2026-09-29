//! Typed recipes provide geometry and native states, never motion shortcuts.
use crate::assembly_transform::AssemblyTransform;
use crate::piston_construction::electrical_snapshot;
use crate::snapshot::assembly_from_snapshot;
use crate::{MinecraftSnapshot, MinecraftSnapshotBlock, Pos, Region};
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_library::flying_machine::{FlyingMachineMaterial, FlyingMachineRequest};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use std::collections::BTreeSet;

use super::definitions::{EngineDefinition, Part, State, definition};
use dustroute_minecraft::Facing;

fn native(p: Pos, name: &str, properties: &[(&str, &str)]) -> MinecraftSnapshotBlock {
    MinecraftSnapshotBlock {
        pos: p,
        name: format!("minecraft:{name}"),
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}
fn facing_name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}
impl Part {
    fn native(self) -> MinecraftSnapshotBlock {
        let p = self.position;
        match self.state {
            State::Piston { facing, sticky } => native(
                p,
                if sticky { "sticky_piston" } else { "piston" },
                &[("facing", facing_name(facing)), ("extended", "false")],
            ),
            State::Observer { watching } => native(
                p,
                "observer",
                &[("facing", facing_name(watching)), ("powered", "false")],
            ),
            State::Material(m) => native(
                p,
                match m {
                    FlyingMachineMaterial::Stone => "stone",
                    FlyingMachineMaterial::Glass => "glass",
                    FlyingMachineMaterial::Slime => "slime_block",
                    FlyingMachineMaterial::Honey => "honey_block",
                },
                &[],
            ),
            State::WallLever { facing, powered } => native(
                p,
                "lever",
                &[
                    ("face", "wall"),
                    ("facing", facing_name(facing)),
                    ("powered", if powered { "true" } else { "false" }),
                ],
            ),
            State::Obsidian => native(p, "obsidian", &[]),
        }
    }
}

pub(super) struct Recipe {
    pub initial: MinecraftSnapshot,
    pub arrival: MinecraftSnapshot,
    pub moving: Vec<Pos>,
    pub delta: Pos,
    pub context: RuntimeBehaviorContext,
    pub sweep: BTreeSet<Pos>,
}

pub(super) fn expand(request: &FlyingMachineRequest) -> Result<Recipe, String> {
    expand_definition(request, definition(request.engine))
}

fn expand_definition(
    request: &FlyingMachineRequest,
    engine: &EngineDefinition,
) -> Result<Recipe, String> {
    if request.namespace.is_empty()
        || request.namespace.len() > 64
        || !request
            .namespace
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(
            "namespace needs 1..64 ASCII letters, digits, dot, underscore or hyphen".into(),
        );
    }
    if !(1..=16).contains(&request.distance) || request.attachments.len() > 12 {
        return Err("generation supports distances 1..16 and at most 12 additional blocks".into());
    }
    let body = engine
        .bodies
        .iter()
        .find(|(body, _)| *body == request.body)
        .map(|(_, parts)| *parts)
        .ok_or("body is not defined for this engine")?;
    let mut parts: Vec<_> = engine.moving.iter().chain(body).copied().collect();
    for extra in &request.attachments {
        let p = extra.position;
        if !(-4..=4).contains(&p.x) || !(-2..=3).contains(&p.y) || !(-5..=5).contains(&p.z) {
            return Err("attachment coordinates exceed the bounded source envelope".into());
        }
        parts.push(Part {
            position: p,
            state: State::Material(extra.material),
        });
    }
    let moving: BTreeSet<_> = parts.iter().map(|p| p.position).collect();
    if moving.len() != parts.len() {
        return Err("duplicate engine/body/attachment position".into());
    }
    let distance = i32::from(request.distance);
    // All engines declare a contact row. Geometry proposes a stopper, and the
    // ordinary physical verifier decides whether that stops the whole machine.
    let front = parts
        .iter()
        .filter(|p| (p.position.y, p.position.z) == engine.stopper_lane)
        .map(|p| p.position.x)
        .max()
        .ok_or("engine stopper contact row is empty")?;
    let control = engine.launch;
    let mut declared: Vec<_> = parts
        .into_iter()
        .chain(engine.fixed.iter().copied())
        .collect();
    declared.push(Part {
        position: Pos::new(
            front + distance + 1,
            engine.stopper_lane.0,
            engine.stopper_lane.1,
        ),
        state: State::Obsidian,
    });
    let positions: BTreeSet<_> = declared.iter().map(|p| p.position).collect();
    if positions.len() != declared.len() {
        return Err("moving blocks overlap the launcher or stopper".into());
    }
    if moving.contains(&control)
        || !declared.iter().any(|p| {
            p.position == control && matches!(p.state, State::WallLever { powered: false, .. })
        })
    {
        return Err("engine requires a fixed initially OFF launch lever".into());
    }
    let overrides: BTreeSet<_> = engine
        .arrival_overrides
        .iter()
        .map(|p| p.position)
        .collect();
    if overrides.len() != engine.arrival_overrides.len() || !overrides.is_subset(&positions) {
        return Err("arrival overrides must name distinct existing parts".into());
    }
    let blocks: Vec<_> = declared.iter().map(|p| p.native()).collect();
    let mut arrival: Vec<_> = declared
        .iter()
        .map(|p| {
            engine
                .arrival_overrides
                .iter()
                .find(|after| after.position == p.position)
                .unwrap_or(p)
                .native()
        })
        .collect();
    if !arrival.iter().any(|b| {
        b.pos == control
            && b.name == "minecraft:lever"
            && b.properties.get("powered").is_some_and(|v| v == "true")
    }) {
        return Err("arrival must retain the launch lever held ON".into());
    }
    for b in &mut arrival {
        if moving.contains(&b.pos) {
            b.pos.x += distance;
        }
    }
    if arrival.iter().map(|b| b.pos).collect::<BTreeSet<_>>().len() != arrival.len() {
        return Err("declared arrival overlaps fixed equipment".into());
    }
    let mut min = control;
    let mut max = control;
    for p in blocks.iter().chain(&arrival).map(|b| b.pos) {
        min = Pos::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
        max = Pos::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
    }
    let region = Region::new(min.offset(-3, -3, -3), max.offset(3, 3, 3));
    let mirror = |p: Pos| {
        if request.mirrored {
            Pos::new(p.x, p.y, -p.z)
        } else {
            p
        }
    };
    let project = |p: Pos| request.rotation.pos(mirror(p));
    let convert = |mut blocks: Vec<MinecraftSnapshotBlock>| -> Result<MinecraftSnapshot, String> {
        for b in &mut blocks {
            b.pos = mirror(b.pos);
            match b.properties.get_mut("facing") {
                Some(facing) if request.mirrored => match facing.as_str() {
                    "north" => *facing = "south".into(),
                    "south" => *facing = "north".into(),
                    _ => {}
                },
                _ => {}
            }
        }
        let a = mirror(region.min);
        let b = mirror(region.max);
        let mirrored = Region::new(
            Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        );
        let snapshot = MinecraftSnapshot {
            min: mirrored.min,
            max: mirrored.max,
            blocks,
        };
        let assembly = assembly_from_snapshot(&snapshot, "generated flight", vec![mirrored])
            .map_err(|e| e.to_string())?;
        let context = RuntimeBehaviorContext::fresh_pistons(mirrored, vec![mirror(control)]);
        let (assembly, context) = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor: Pos::default(),
            rotation: request.rotation,
        }
        .apply(&assembly, &context)?;
        electrical_snapshot(
            &assembly
                .inspect(&BlueprintCatalog::default())
                .map_err(|e| e.to_string())?
                .proposed_world(),
            context.known_region,
        )
    };
    let initial = convert(blocks)?;
    let arrival = convert(arrival)?;
    // Include every cell swept by each declared moving block, plus stationary
    // equipment. No simulated output supplies these expected endpoints.
    let mut sweep = BTreeSet::new();
    for p in &moving {
        for d in 0..=distance {
            sweep.insert(project(p.offset(d, 0, 0)));
        }
    }
    for b in &initial.blocks {
        sweep.insert(b.pos);
    }
    if sweep.len() > 512 {
        return Err("generated observation set exceeds 512 cells".into());
    }
    let context = RuntimeBehaviorContext::fresh_pistons(
        Region::new(initial.min, initial.max),
        vec![project(control)],
    );
    Ok(Recipe {
        initial,
        arrival,
        moving: moving.into_iter().map(project).collect(),
        delta: request.rotation.pos(Pos::new(distance, 0, 0)),
        context,
        sweep,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_library::flying_machine::FlyingMachineEngine;

    #[test]
    fn unregistered_definition_transforms_facings_and_independent_arrival_states() {
        // A geometry-only fixture: it is deliberately not a working engine.
        // Native directions and endpoint declarations must survive the common
        // projection without invoking an engine-specific branch or simulation.
        const MOVING: &[Part] = &[Part {
            position: Pos::new(0, 0, 1),
            state: State::Observer {
                watching: Facing::North,
            },
        }];
        const ARRIVAL: &[Part] = &[
            Part {
                position: Pos::new(0, 0, 1),
                state: State::Observer {
                    watching: Facing::East,
                },
            },
            Part {
                position: Pos::new(0, 2, 0),
                state: State::WallLever {
                    facing: Facing::East,
                    powered: true,
                },
            },
        ];
        let engine = EngineDefinition {
            moving: MOVING,
            arrival_overrides: ARRIVAL,
            ..*definition(FlyingMachineEngine::SlimeRelay)
        };
        let request: FlyingMachineRequest = serde_json::from_value(serde_json::json!({
            "namespace":"geometry", "distance":4, "rotation":"r90", "mirrored":true
        }))
        .unwrap();
        let recipe = expand_definition(&request, &engine).unwrap();
        let initial = recipe
            .initial
            .blocks
            .iter()
            .find(|b| b.name == "minecraft:observer")
            .unwrap();
        let arrived = recipe
            .arrival
            .blocks
            .iter()
            .find(|b| b.name == "minecraft:observer")
            .unwrap();
        assert_eq!(initial.pos, Pos::new(1, 0, 0));
        assert_eq!(initial.properties["facing"], "west");
        assert_eq!(arrived.pos, Pos::new(1, 0, 4));
        assert_eq!(arrived.properties["facing"], "south");
        let invalid = EngineDefinition {
            arrival_overrides: &[],
            ..engine
        };
        assert!(
            expand_definition(&request, &invalid)
                .err()
                .unwrap()
                .contains("held ON")
        );
    }
}
