use crate::minecraft_export::{
    DataPack, JavaExportConfig, MinecraftExportError, isolated_build_commands,
};
use crate::wire::update_wire_shapes;
use crate::world::{Block, BlockKind, Facing, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticProbe {
    pub name: &'static str,
    pub description: &'static str,
    pub world: World,
    pub stimulus: Vec<String>,
    pub checks: Vec<String>,
}

fn coordinate(value: i32) -> String {
    if value == 0 {
        "~".into()
    } else {
        format!("~{value}")
    }
}

fn xyz(pos: Pos) -> String {
    format!(
        "{} {} {}",
        coordinate(pos.x),
        coordinate(pos.y),
        coordinate(pos.z)
    )
}

fn setblock(pos: Pos, state: &str) -> String {
    format!("setblock {} {state} replace", xyz(pos))
}

fn wire_check(pos: Pos, power: Option<u8>, expected_on: bool, label: &str) -> Vec<String> {
    let state = power.map_or_else(
        || "minecraft:redstone_wire[power=0]".into(),
        |value| format!("minecraft:redstone_wire[power={value}]"),
    );
    let condition = if power.is_some() || !expected_on {
        "if"
    } else {
        "unless"
    };
    let inverse = if condition == "if" { "unless" } else { "if" };
    vec![
        format!(
            "execute {condition} block {} {state} run tellraw @a {{\"text\":\"PASS {label}\",\"color\":\"green\"}}",
            xyz(pos)
        ),
        format!(
            "execute {inverse} block {} {state} run tellraw @a {{\"text\":\"FAIL {label}\",\"color\":\"red\"}}",
            xyz(pos)
        ),
    ]
}

fn torch_check(pos: Pos, lit: bool, label: &str) -> Vec<String> {
    vec![
        format!(
            "execute if block {} minecraft:redstone_wall_torch[lit={lit}] run tellraw @a {{\"text\":\"PASS {label}\",\"color\":\"green\"}}",
            xyz(pos)
        ),
        format!(
            "execute unless block {} minecraft:redstone_wall_torch[lit={lit}] run tellraw @a {{\"text\":\"FAIL {label}\",\"color\":\"red\"}}",
            xyz(pos)
        ),
    ]
}

fn supports(world: &mut World, positions: impl IntoIterator<Item = Pos>) {
    for pos in positions {
        world.set(pos, Block::new(BlockKind::Solid));
    }
}

fn wire(world: &mut World, pos: Pos) {
    world.place(BlockKind::RedstoneWire, pos);
}

fn repeater(world: &mut World, pos: Pos, facing: Facing) {
    let block = world.place(BlockKind::Repeater, pos);
    block.facing = Some(facing);
    block.delay = Some(1);
}

fn torch(world: &mut World, pos: Pos, support_offset: Pos) {
    let block = world.place(BlockKind::RedstoneTorch, pos);
    block.facing = Some(Facing::East);
    block.support_offset = Some(support_offset);
}

fn probe(
    name: &'static str,
    description: &'static str,
    mut world: World,
    stimulus: Vec<String>,
    checks: Vec<String>,
) -> SemanticProbe {
    update_wire_shapes(&mut world);
    SemanticProbe {
        name,
        description,
        world,
        stimulus,
        checks,
    }
}

#[must_use]
pub fn semantic_probes() -> Vec<SemanticProbe> {
    let mut probes = Vec::new();

    let mut w = World::new();
    supports(&mut w, [Pos::new(1, 0, 0)]);
    wire(&mut w, Pos::new(1, 1, 0));
    probes.push(probe(
        "01_source_to_dust",
        "source directly drives dust",
        w,
        vec![setblock(Pos::new(0, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(1, 1, 0), Some(15), true, "source -> dust"),
    ));

    let mut w = World::new();
    supports(&mut w, (1..4).map(|x| Pos::new(x, 0, 0)));
    for x in 1..4 {
        wire(&mut w, Pos::new(x, 1, 0));
    }
    let mut checks = Vec::new();
    for (x, power) in [(1, 15), (2, 14), (3, 13)] {
        checks.extend(wire_check(
            Pos::new(x, 1, 0),
            Some(power),
            true,
            "dust decay",
        ));
    }
    probes.push(probe(
        "02_dust_decay",
        "dust propagates 15,14,13",
        w,
        vec![setblock(Pos::new(0, 1, 0), "minecraft:redstone_block")],
        checks,
    ));

    let mut w = World::new();
    supports(&mut w, [Pos::new(1, 0, 0), Pos::new(-1, 0, 0)]);
    wire(&mut w, Pos::new(1, 1, 0));
    w.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(-1, 1, 0));
    probes.push(probe(
        "03_weak_block_no_dust_return",
        "weak block power does not emerge as dust",
        w,
        vec![setblock(Pos::new(2, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(-1, 1, 0), None, false, "weak block isolation"),
    ));

    let mut w = World::new();
    supports(
        &mut w,
        [Pos::new(1, 0, 0), Pos::new(-1, 0, 0), Pos::new(-2, 0, 0)],
    );
    wire(&mut w, Pos::new(1, 1, 0));
    w.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
    repeater(&mut w, Pos::new(-1, 1, 0), Facing::West);
    wire(&mut w, Pos::new(-2, 1, 0));
    probes.push(probe(
        "04_weak_block_to_repeater",
        "repeater reads weak-powered block",
        w,
        vec![setblock(Pos::new(2, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(-2, 1, 0), None, true, "weak block -> repeater"),
    ));

    let mut w = World::new();
    supports(&mut w, (0..5).map(|x| Pos::new(x, 0, 0)));
    for x in 0..3 {
        wire(&mut w, Pos::new(x, 1, 0));
    }
    repeater(&mut w, Pos::new(3, 1, 0), Facing::East);
    wire(&mut w, Pos::new(4, 1, 0));
    probes.push(probe(
        "05_repeater_refresh",
        "repeater restores signal to 15",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(4, 1, 0), Some(15), true, "repeater refresh"),
    ));

    let mut w = World::new();
    supports(
        &mut w,
        [Pos::new(0, 0, 0), Pos::new(1, 0, 0), Pos::new(3, 0, 0)],
    );
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    w.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(3, 1, 0));
    probes.push(probe(
        "06_repeater_strong_block",
        "repeater strongly powers block",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(3, 1, 0), None, true, "strong block -> dust"),
    ));

    let mut w = World::new();
    w.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(1, 0, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "07_torch_unpowered_support",
        "unpowered support leaves torch lit",
        w,
        vec![],
        torch_check(Pos::new(1, 0, 0), true, "torch on"),
    ));

    let mut w = World::new();
    w.set(Pos::new(-1, 0, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(-1, 1, 0));
    w.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(1, 1, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "08_torch_powered_support",
        "dust-powered support turns torch off",
        w,
        vec![setblock(Pos::new(-2, 1, 0), "minecraft:redstone_block")],
        torch_check(Pos::new(1, 1, 0), false, "torch off"),
    ));

    let mut w = World::new();
    w.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(1, 0, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "09_redstone_block_no_block_propagation",
        "source block does not create stored power in adjacent solid",
        w,
        vec![setblock(Pos::new(-1, 0, 0), "minecraft:redstone_block")],
        torch_check(Pos::new(1, 0, 0), true, "torch stays on"),
    ));

    let mut w = World::new();
    supports(&mut w, (0..3).map(|x| Pos::new(x, 0, 0)));
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    wire(&mut w, Pos::new(2, 1, 0));
    probes.push(probe(
        "10_dust_repeater_dust",
        "canonical dust repeater dust",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(2, 1, 0), Some(15), true, "repeater boundary"),
    ));

    let mut w = World::new();
    supports(&mut w, (0..3).map(|x| Pos::new(x, 0, 0)));
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    wire(&mut w, Pos::new(2, 1, 0));
    probes.push(probe(
        "11_repeater_reverse_blocked",
        "repeater blocks reverse flow",
        w,
        vec![setblock(Pos::new(3, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(0, 1, 0), None, false, "reverse blocked"),
    ));

    let mut w = World::new();
    supports(&mut w, (0..4).map(|x| Pos::new(x, 0, 0)));
    wire(&mut w, Pos::new(0, 1, 0));
    wire(&mut w, Pos::new(1, 1, 0));
    repeater(&mut w, Pos::new(2, 1, 0), Facing::East);
    wire(&mut w, Pos::new(3, 1, 0));
    probes.push(probe(
        "12_dust_to_repeater_input",
        "dust enters repeater input",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(3, 1, 0), None, true, "repeater input"),
    ));

    let mut w = World::new();
    supports(
        &mut w,
        [Pos::new(0, 0, 0), Pos::new(1, 0, 0), Pos::new(1, 0, 1)],
    );
    for pos in [Pos::new(0, 1, 0), Pos::new(1, 1, 0), Pos::new(1, 1, 1)] {
        wire(&mut w, pos);
    }
    probes.push(probe(
        "13_dust_corner",
        "dust carries around corner",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(1, 1, 1), None, true, "dust corner"),
    ));

    let mut w = World::new();
    w.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    w.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(0, 1, 0));
    wire(&mut w, Pos::new(1, 2, 0));
    probes.push(probe(
        "14_dust_stair_up",
        "dust climbs one block",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(1, 2, 0), None, true, "stair up"),
    ));

    let mut w = World::new();
    w.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
    w.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(0, 2, 0));
    wire(&mut w, Pos::new(1, 1, 0));
    probes.push(probe(
        "15_dust_stair_down",
        "dust descends one block",
        w,
        vec![setblock(Pos::new(-1, 2, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(1, 1, 0), None, true, "stair down"),
    ));

    let mut w = World::new();
    w.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    wire(&mut w, Pos::new(0, 1, 0));
    w.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(2, 1, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "16_leaf_dust_block_power",
        "leaf dust powers block input",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        torch_check(Pos::new(2, 1, 0), false, "leaf block power"),
    ));

    let mut w = World::new();
    supports(
        &mut w,
        [
            Pos::new(0, 0, 0),
            Pos::new(1, 0, 0),
            Pos::new(2, 0, 0),
            Pos::new(1, 0, 1),
        ],
    );
    for pos in [
        Pos::new(0, 1, 0),
        Pos::new(1, 1, 0),
        Pos::new(2, 1, 0),
        Pos::new(1, 1, 1),
    ] {
        wire(&mut w, pos);
    }
    w.set(Pos::new(3, 1, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(4, 1, 0), Pos::new(-1, 0, 0));
    let mut checks = wire_check(Pos::new(1, 1, 1), None, true, "fanout branch");
    checks.extend(torch_check(Pos::new(4, 1, 0), false, "fanout leaf"));
    probes.push(probe(
        "17_branch_leaf_block_power",
        "fanout feeds leaf block input",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        checks,
    ));

    let mut w = World::new();
    supports(&mut w, (0..5).map(|x| Pos::new(x, 0, 0)));
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    wire(&mut w, Pos::new(2, 1, 0));
    wire(&mut w, Pos::new(3, 1, 0));
    w.set(Pos::new(4, 1, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(5, 1, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "18_repeater_route_block_power",
        "refreshed route powers block input",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        torch_check(Pos::new(5, 1, 0), false, "refreshed block power"),
    ));

    let mut w = World::new();
    supports(&mut w, (0..6).map(|x| Pos::new(x, 0, 0)));
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    for x in 2..5 {
        wire(&mut w, Pos::new(x, 1, 0));
    }
    w.set(Pos::new(5, 1, 0), Block::new(BlockKind::Solid));
    torch(&mut w, Pos::new(6, 1, 0), Pos::new(-1, 0, 0));
    probes.push(probe(
        "19_cell_output_to_block_input",
        "cell output crosses route boundary",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        torch_check(Pos::new(6, 1, 0), false, "cell boundary"),
    ));

    let mut w = World::new();
    supports(
        &mut w,
        [
            Pos::new(0, 0, 0),
            Pos::new(1, 0, 0),
            Pos::new(2, 0, 0),
            Pos::new(2, 0, 1),
            Pos::new(2, 0, 2),
        ],
    );
    wire(&mut w, Pos::new(0, 1, 0));
    repeater(&mut w, Pos::new(1, 1, 0), Facing::East);
    for z in 0..3 {
        wire(&mut w, Pos::new(2, 1, z));
    }
    probes.push(probe(
        "20_repeater_to_corner",
        "repeater output enters dust corner",
        w,
        vec![setblock(Pos::new(-1, 1, 0), "minecraft:redstone_block")],
        wire_check(Pos::new(2, 1, 2), None, true, "repeater corner"),
    ));

    probes
}

pub fn semantics_datapack(config: &JavaExportConfig) -> Result<DataPack, MinecraftExportError> {
    let probes = semantic_probes();
    let mut pack = DataPack::default();
    pack.insert_text("pack.mcmeta", format!("{{\"pack\":{{\"description\":\"DustRoute Rust semantics 01-20\",\"min_format\":[{},{}],\"max_format\":[{},{}]}}}}\n", config.pack_format, config.pack_format_minor, config.pack_format, config.pack_format_minor));
    let mut suite = Vec::new();
    let window = config.settle_ticks + 12;
    for (index, probe) in probes.iter().enumerate() {
        let root = format!("data/{}/function/{}", config.namespace, probe.name);
        let tag = format!("{}_{}_origin", config.namespace, probe.name);
        pack.insert_text(
            format!("{root}/_build.mcfunction"),
            isolated_build_commands(&probe.world, config)?.join("\n") + "\n",
        );
        pack.insert_text(
            format!("{root}/_stimulate.mcfunction"),
            probe.stimulus.join("\n") + "\n",
        );
        pack.insert_text(
            format!("{root}/_check.mcfunction"),
            probe.checks.join("\n") + "\n",
        );
        pack.insert_text(format!("{root}/build.mcfunction"), format!("kill @e[type=minecraft:marker,tag={tag}]\nsummon minecraft:marker ~ ~ ~ {{Tags:[\"{tag}\"]}}\nexecute at @e[type=minecraft:marker,tag={tag},limit=1] run function {}:{}/_build\n", config.namespace, probe.name));
        for action in ["stimulate", "check"] {
            pack.insert_text(format!("{root}/{action}.mcfunction"), format!("execute at @e[type=minecraft:marker,tag={tag},limit=1] run function {}:{}/_{action}\n", config.namespace, probe.name));
        }
        pack.insert_text(format!("{root}/run.mcfunction"), format!("function {}:{}/build\nschedule function {}:{}/stimulate 4t replace\nschedule function {}:{}/check {}t replace\n", config.namespace, probe.name, config.namespace, probe.name, config.namespace, probe.name, config.settle_ticks + 4));
        suite.push(format!(
            "schedule function {}:{}/run {}t replace",
            config.namespace,
            probe.name,
            2 + u32::try_from(index).expect("probe index fits u32") * window
        ));
    }
    suite.push(format!(
        "schedule function {}:complete {}t replace",
        config.namespace,
        2 + u32::try_from(probes.len()).expect("probe count fits u32") * window
    ));
    pack.insert_text(
        format!("data/{}/function/complete.mcfunction", config.namespace),
        "tellraw @a {\"text\":\"DUSTROUTE COMPLETE\",\"color\":\"aqua\"}\n",
    );
    pack.insert_text(
        format!("data/{}/function/tests.mcfunction", config.namespace),
        suite.join("\n") + "\n",
    );
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_and_suite_cover_all_twenty_probes() {
        let probes = semantic_probes();
        assert_eq!(probes.len(), 20);
        assert_eq!(probes.first().unwrap().name, "01_source_to_dust");
        assert_eq!(probes.last().unwrap().name, "20_repeater_to_corner");
        let config = JavaExportConfig {
            namespace: "ro_sem".into(),
            ..JavaExportConfig::default()
        };
        let pack = semantics_datapack(&config).unwrap();
        assert!(
            pack.files
                .contains_key("data/ro_sem/function/tests.mcfunction")
        );
        assert!(
            pack.files
                .contains_key("data/ro_sem/function/14_dust_stair_up/run.mcfunction")
        );
        assert!(
            pack.files
                .contains_key("data/ro_sem/function/20_repeater_to_corner/check.mcfunction")
        );
        assert!(
            pack.files
                .get("data/ro_sem/function/tests.mcfunction")
                .unwrap()
                .windows(b"schedule function ro_sem:complete".len())
                .any(|window| window == b"schedule function ro_sem:complete")
        );
    }

    #[test]
    fn weak_power_and_source_block_probes_keep_intended_geometry() {
        let probes = semantic_probes();
        let weak = probes
            .iter()
            .find(|probe| probe.name.starts_with("03_"))
            .unwrap();
        assert_eq!(weak.world.kind_at(Pos::new(0, 1, 0)), BlockKind::Solid);
        assert_eq!(weak.world.kind_at(Pos::new(0, 2, 0)), BlockKind::Air);
        let source = probes
            .iter()
            .find(|probe| probe.name.starts_with("09_"))
            .unwrap();
        assert!(
            source
                .stimulus
                .iter()
                .any(|line| line.contains("redstone_block"))
        );
    }
}
