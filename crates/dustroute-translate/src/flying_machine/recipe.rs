//! Typed recipes provide geometry and native states, never motion shortcuts.
use crate::assembly_transform::AssemblyTransform;
use crate::piston_construction::electrical_snapshot;
use crate::snapshot::assembly_from_snapshot;
use crate::{MinecraftSnapshot, MinecraftSnapshotBlock, Pos, Region};
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_library::flying_machine::{
    FlyingMachineBody, FlyingMachineMaterial, FlyingMachineRequest,
};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
enum PartKind {
    Push,
    Pull,
    Observer,
    Material(FlyingMachineMaterial),
}
#[derive(Clone, Copy)]
struct Part {
    p: Pos,
    kind: PartKind,
}
use FlyingMachineMaterial::{Glass, Honey, Slime, Stone};
const ENGINE: &[Part] = &[
    Part {
        p: Pos::new(0, 0, 0),
        kind: PartKind::Material(Slime),
    },
    Part {
        p: Pos::new(0, 0, 1),
        kind: PartKind::Push,
    },
    Part {
        p: Pos::new(0, 1, 0),
        kind: PartKind::Observer,
    },
    Part {
        p: Pos::new(1, 0, 0),
        kind: PartKind::Pull,
    },
    Part {
        p: Pos::new(1, 0, 1),
        kind: PartKind::Material(Slime),
    },
    Part {
        p: Pos::new(1, 1, 1),
        kind: PartKind::Observer,
    },
];
const SIDES: &[Part] = &[
    Part {
        p: Pos::new(0, 0, -1),
        kind: PartKind::Material(Stone),
    },
    Part {
        p: Pos::new(1, 0, 2),
        kind: PartKind::Material(Stone),
    },
];
const WINGS: &[Part] = &[
    Part {
        p: Pos::new(0, 0, -1),
        kind: PartKind::Material(Slime),
    },
    Part {
        p: Pos::new(1, 0, 2),
        kind: PartKind::Material(Slime),
    },
    Part {
        p: Pos::new(0, 0, -2),
        kind: PartKind::Material(Stone),
    },
    Part {
        p: Pos::new(1, 0, 3),
        kind: PartKind::Material(Stone),
    },
];
const NOSE: &[Part] = &[
    Part {
        p: Pos::new(2, 0, 1),
        kind: PartKind::Material(Honey),
    },
    Part {
        p: Pos::new(2, 0, 2),
        kind: PartKind::Material(Glass),
    },
];
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
impl Part {
    fn native(self) -> MinecraftSnapshotBlock {
        match self.kind {
            PartKind::Push => native(
                self.p,
                "piston",
                &[("facing", "east"), ("extended", "false")],
            ),
            PartKind::Pull => native(
                self.p,
                "sticky_piston",
                &[("facing", "west"), ("extended", "false")],
            ),
            PartKind::Observer => native(
                self.p,
                "observer",
                &[("facing", "up"), ("powered", "false")],
            ),
            PartKind::Material(m) => native(
                self.p,
                match m {
                    Stone => "stone",
                    Glass => "glass",
                    Slime => "slime_block",
                    Honey => "honey_block",
                },
                &[],
            ),
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
    let body = match request.body {
        FlyingMachineBody::Compact => &[][..],
        FlyingMachineBody::SideBlocks => SIDES,
        FlyingMachineBody::SlimeWings => WINGS,
        FlyingMachineBody::HoneyNose => NOSE,
    };
    let mut parts: Vec<_> = ENGINE.iter().chain(body).copied().collect();
    for extra in &request.attachments {
        let p = extra.position;
        if !(-4..=4).contains(&p.x) || !(-2..=3).contains(&p.y) || !(-5..=5).contains(&p.z) {
            return Err("attachment coordinates exceed the bounded source envelope".into());
        }
        parts.push(Part {
            p,
            kind: PartKind::Material(extra.material),
        });
    }
    let moving: BTreeSet<_> = parts.iter().map(|p| p.p).collect();
    if moving.len() != parts.len() {
        return Err("duplicate engine/body/attachment position".into());
    }
    let distance = i32::from(request.distance);
    // Stop the leading material in the pushing row. The physical verifier,
    // not this geometric rule, establishes whether the whole machine arrives.
    let front = parts
        .iter()
        .filter(|p| p.p.y == 0 && p.p.z == 1)
        .map(|p| p.p.x)
        .max()
        .unwrap();
    let control = Pos::new(0, 2, 0);
    let mut blocks: Vec<_> = parts.into_iter().map(Part::native).collect();
    blocks.extend([
        native(Pos::new(-1, 2, 0), "stone", &[]),
        native(
            control,
            "lever",
            &[("face", "wall"), ("facing", "east"), ("powered", "false")],
        ),
        native(Pos::new(front + distance + 1, 0, 1), "obsidian", &[]),
    ]);
    if blocks.iter().map(|b| b.pos).collect::<BTreeSet<_>>().len() != blocks.len() {
        return Err("moving blocks overlap the launcher or stopper".into());
    }
    let mut arrival = blocks.clone();
    for b in &mut arrival {
        if moving.contains(&b.pos) {
            b.pos.x += distance;
        }
        if b.pos == control && b.name == "minecraft:lever" {
            b.properties.insert("powered".into(), "true".into());
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
