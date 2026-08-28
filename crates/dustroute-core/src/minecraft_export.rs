use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::compiler::BaselineCompileResult;
use crate::logic::LogicError;
use crate::world::{Block, BlockKind, Facing, Pos, WireConnection, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaExportConfig {
    pub namespace: String,
    pub solid_block: String,
    pub transparent_block: String,
    pub relative: bool,
    pub pack_format: u32,
    pub pack_format_minor: u32,
    pub settle_ticks: u32,
    pub reset_margin: i32,
}

impl Default for JavaExportConfig {
    fn default() -> Self {
        Self {
            namespace: "dustroute".into(),
            solid_block: "minecraft:stone".into(),
            transparent_block: "minecraft:glass".into(),
            relative: true,
            pack_format: 94,
            pack_format_minor: 1,
            settle_ticks: 60,
            reset_margin: 3,
        }
    }
}

#[derive(Debug)]
pub enum MinecraftExportError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    Logic(LogicError),
    InvalidNamespace(String),
    InvalidResourceName(String),
    TooManyInputs(usize),
    UnsupportedFacing(Facing),
}

impl Display for MinecraftExportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::Zip(error) => Display::fmt(error, f),
            Self::Logic(error) => Display::fmt(error, f),
            Self::InvalidNamespace(namespace) => write!(f, "invalid namespace: {namespace}"),
            Self::InvalidResourceName(name) => write!(f, "invalid resource name: {name}"),
            Self::TooManyInputs(count) => write!(
                f,
                "cannot export exhaustive tests for {count} inputs (maximum 16)"
            ),
            Self::UnsupportedFacing(facing) => {
                write!(f, "unsupported horizontal facing: {facing:?}")
            }
        }
    }
}

impl Error for MinecraftExportError {}
impl From<io::Error> for MinecraftExportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<zip::result::ZipError> for MinecraftExportError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}
impl From<LogicError> for MinecraftExportError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
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

fn wire_connection_name(connection: WireConnection) -> &'static str {
    match connection {
        WireConnection::None => "none",
        WireConnection::Side => "side",
        WireConnection::Up => "up",
    }
}

fn outward_from_support(offset: Pos) -> Option<Facing> {
    match (-offset.x, -offset.y, -offset.z) {
        (0, 0, -1) => Some(Facing::North),
        (1, 0, 0) => Some(Facing::East),
        (0, 0, 1) => Some(Facing::South),
        (-1, 0, 0) => Some(Facing::West),
        _ => None,
    }
}

pub fn java_block_state(
    block: &Block,
    config: &JavaExportConfig,
) -> Result<String, MinecraftExportError> {
    let state = match block.kind {
        BlockKind::Air => "minecraft:air".into(),
        BlockKind::Solid => config.solid_block.clone(),
        BlockKind::Transparent => config.transparent_block.clone(),
        BlockKind::RedstoneBlock => "minecraft:redstone_block".into(),
        BlockKind::RedstoneWire => {
            let connections = block.wire_connections.as_ref();
            let values = [Facing::North, Facing::East, Facing::South, Facing::West]
                .map(|facing| {
                    let connection = connections
                        .and_then(|states| states.get(&facing))
                        .copied()
                        .unwrap_or(WireConnection::None);
                    format!(
                        "{}={}",
                        facing_name(facing),
                        wire_connection_name(connection)
                    )
                })
                .join(",");
            format!("minecraft:redstone_wire[{values},power=0]")
        }
        BlockKind::Repeater | BlockKind::Comparator => {
            let facing = block.facing.unwrap_or(Facing::North).opposite();
            if facing.horizontal_offset().is_none() {
                return Err(MinecraftExportError::UnsupportedFacing(facing));
            }
            if block.kind == BlockKind::Repeater {
                format!(
                    "minecraft:repeater[delay={},facing={},locked=false,powered=false]",
                    block.delay.unwrap_or(1).clamp(1, 4),
                    facing_name(facing)
                )
            } else {
                format!(
                    "minecraft:comparator[facing={},mode=compare,powered=false]",
                    facing_name(facing)
                )
            }
        }
        BlockKind::RedstoneTorch => {
            if let Some(facing) = block.support_offset.and_then(outward_from_support) {
                format!(
                    "minecraft:redstone_wall_torch[facing={},lit=true]",
                    facing_name(facing)
                )
            } else {
                "minecraft:redstone_torch[lit=true]".into()
            }
        }
        BlockKind::Lever => {
            let powered = if block.powered.unwrap_or(false) {
                "true"
            } else {
                "false"
            };
            let (face, facing) = match block.support_offset {
                Some(offset) if offset.y < 0 => ("floor", block.facing.unwrap_or(Facing::North)),
                Some(offset) if offset.y > 0 => ("ceiling", block.facing.unwrap_or(Facing::North)),
                Some(offset) => (
                    "wall",
                    outward_from_support(offset).unwrap_or(Facing::North),
                ),
                None => ("floor", block.facing.unwrap_or(Facing::North)),
            };
            format!(
                "minecraft:lever[face={face},facing={},powered={powered}]",
                facing_name(facing)
            )
        }
        BlockKind::Piston => format!(
            "minecraft:piston[facing={},extended=false]",
            facing_name(block.facing.unwrap_or(Facing::North))
        ),
    };
    Ok(state)
}

fn coordinate(value: i32, relative: bool) -> String {
    if relative {
        if value == 0 {
            "~".into()
        } else {
            format!("~{value}")
        }
    } else {
        value.to_string()
    }
}

fn xyz(pos: Pos, config: &JavaExportConfig) -> String {
    format!(
        "{} {} {}",
        coordinate(pos.x, config.relative),
        coordinate(pos.y, config.relative),
        coordinate(pos.z, config.relative)
    )
}

pub fn world_setblock_commands(
    world: &World,
    config: &JavaExportConfig,
) -> Result<Vec<String>, MinecraftExportError> {
    let priority = |kind| match kind {
        BlockKind::Solid
        | BlockKind::Transparent
        | BlockKind::RedstoneBlock
        | BlockKind::Piston => 0,
        BlockKind::RedstoneTorch | BlockKind::Lever => 2,
        _ => 1,
    };
    let mut items: Vec<_> = world.iter().collect();
    items.sort_by_key(|(pos, block)| (priority(block.kind), pos.y, pos.x, pos.z));
    items
        .into_iter()
        .map(|(pos, block)| {
            Ok(format!(
                "setblock {} {} replace",
                xyz(*pos, config),
                java_block_state(block, config)?
            ))
        })
        .collect()
}

pub fn isolated_build_commands(
    world: &World,
    config: &JavaExportConfig,
) -> Result<Vec<String>, MinecraftExportError> {
    let Some((world_low, world_high)) = world.bounds() else {
        return Ok(Vec::new());
    };
    let low = world_low.offset(-config.reset_margin, -2, -config.reset_margin);
    let high = world_high.offset(config.reset_margin, 3, config.reset_margin);
    let regions = split_fill_regions(low, high);
    let mut commands = Vec::new();
    for block in [
        "minecraft:redstone_wire",
        "minecraft:repeater",
        "minecraft:comparator",
        "minecraft:redstone_torch",
        "minecraft:redstone_wall_torch",
        "minecraft:lever",
    ] {
        commands.extend(regions.iter().map(|(a, b)| {
            format!(
                "fill {} {} minecraft:air replace {block}",
                xyz(*a, config),
                xyz(*b, config)
            )
        }));
    }
    commands.extend(regions.iter().map(|(a, b)| {
        format!(
            "fill {} {} minecraft:air replace",
            xyz(*a, config),
            xyz(*b, config)
        )
    }));
    let foundation_low = Pos::new(low.x, world_low.y - 1, low.z);
    let foundation_high = Pos::new(high.x, world_low.y - 1, high.z);
    commands.extend(
        split_fill_regions(foundation_low, foundation_high)
            .into_iter()
            .map(|(a, b)| {
                format!(
                    "fill {} {} minecraft:stone replace",
                    xyz(a, config),
                    xyz(b, config)
                )
            }),
    );
    commands.extend(world_setblock_commands(world, config)?);
    Ok(commands)
}

fn split_fill_regions(low: Pos, high: Pos) -> Vec<(Pos, Pos)> {
    const EDGE: i32 = 32;
    let mut regions = Vec::new();
    let mut x = low.x;
    while x <= high.x {
        let x_end = high.x.min(x.saturating_add(EDGE - 1));
        let mut y = low.y;
        while y <= high.y {
            let y_end = high.y.min(y.saturating_add(EDGE - 1));
            let mut z = low.z;
            while z <= high.z {
                let z_end = high.z.min(z.saturating_add(EDGE - 1));
                regions.push((Pos::new(x, y, z), Pos::new(x_end, y_end, z_end)));
                z = z_end.saturating_add(1);
            }
            y = y_end.saturating_add(1);
        }
        x = x_end.saturating_add(1);
    }
    regions
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DataPack {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl DataPack {
    pub fn insert_text(&mut self, path: impl Into<String>, text: impl Into<String>) {
        self.files.insert(path.into(), text.into().into_bytes());
    }

    pub fn write_directory(&self, root: &Path) -> Result<(), MinecraftExportError> {
        for (path, contents) in &self.files {
            let destination = root.join(path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(destination, contents)?;
        }
        Ok(())
    }

    pub fn write_zip(&self, path: &Path) -> Result<PathBuf, MinecraftExportError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let mut archive = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, contents) in &self.files {
            archive.start_file(name, options)?;
            archive.write_all(contents)?;
        }
        archive.finish()?;
        Ok(path.to_path_buf())
    }
}

fn validate_namespace(namespace: &str) -> Result<(), MinecraftExportError> {
    if namespace.is_empty()
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
    {
        return Err(MinecraftExportError::InvalidNamespace(namespace.into()));
    }
    Ok(())
}

fn validate_resource_name(name: &str) -> Result<(), MinecraftExportError> {
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(MinecraftExportError::InvalidResourceName(name.into()));
    }
    Ok(())
}

pub fn compiled_circuit_datapack(
    compiled: &BaselineCompileResult,
    circuit_name: &str,
    config: &JavaExportConfig,
) -> Result<DataPack, MinecraftExportError> {
    validate_namespace(&config.namespace)?;
    validate_resource_name(circuit_name)?;
    let root = format!("data/{}/function/{circuit_name}", config.namespace);
    let mut pack = DataPack::default();
    pack.insert_text("pack.mcmeta", format!("{{\n  \"pack\": {{\n    \"description\": \"DustRoute Rust validation\",\n    \"min_format\": [{}, {}],\n    \"max_format\": [{}, {}]\n  }}\n}}\n", config.pack_format, config.pack_format_minor, config.pack_format, config.pack_format_minor));
    let build = isolated_build_commands(&compiled.world, config)?.join("\n") + "\n";
    pack.insert_text(format!("{root}/_build.mcfunction"), build);
    let tag = format!("{}_{}_origin", config.namespace, circuit_name);
    pack.insert_text(format!("{root}/build.mcfunction"), format!(
        "kill @e[type=minecraft:marker,tag={tag}]\nsummon minecraft:marker ~ ~ ~ {{Tags:[\"{tag}\"]}}\nexecute at @e[type=minecraft:marker,tag={tag},limit=1] run function {}:{circuit_name}/_build\n",
        config.namespace,
    ));
    let input_names: Vec<_> = compiled.input_positions.keys().cloned().collect();
    if input_names.len() > 16 {
        return Err(MinecraftExportError::TooManyInputs(input_names.len()));
    }
    let cases = 1_usize << input_names.len();
    let mut test_lines = vec![format!(
        "function {}:{circuit_name}/build",
        config.namespace
    )];
    let window = config.settle_ticks + 12;
    for bits in 0..cases {
        let tag_bits: String = (0..input_names.len())
            .map(|index| if bits & (1 << index) == 0 { '0' } else { '1' })
            .collect();
        let values: BTreeMap<_, _> = input_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), bits & (1 << index) != 0))
            .collect();
        let expected = compiled
            .abstract_dag
            .evaluate(&values.into_iter().collect())?;
        let mut stimulate = Vec::new();
        for (index, name) in input_names.iter().enumerate() {
            let driver = compiled.input_positions[name].offset(-1, 0, 0);
            let block = if bits & (1 << index) == 0 {
                "minecraft:air"
            } else {
                "minecraft:redstone_block"
            };
            stimulate.push(format!("setblock {} {block} replace", xyz(driver, config)));
        }
        stimulate.push(format!(
            "schedule function {}:{circuit_name}/check_{tag_bits} {}t replace",
            config.namespace, config.settle_ticks
        ));
        pack.insert_text(
            format!("{root}/_stimulate_{tag_bits}.mcfunction"),
            stimulate.join("\n") + "\n",
        );
        pack.insert_text(format!("{root}/stimulate_{tag_bits}.mcfunction"), format!("execute at @e[type=minecraft:marker,tag={tag},limit=1] run function {}:{circuit_name}/_stimulate_{tag_bits}\n", config.namespace));
        let checks = expected.iter().flat_map(|(name, value)| {
            let location = xyz(compiled.output_positions[name], config);
            let off = "minecraft:redstone_wire[power=0]";
            let observations = (0..=15)
                .map(|power| format!("execute if block {location} minecraft:redstone_wire[power={power}] run tellraw @a {{\"text\":\"DUSTROUTE OBS {name}=power:{power}\",\"color\":\"gray\"}}"))
                .collect::<Vec<_>>()
                .join("\n");
            if *value {
                let powered = (1..=15)
                    .map(|power| format!("block {location} minecraft:redstone_wire[power={power}]"))
                    .collect::<Vec<_>>();
                let pass_conditions = powered
                    .iter()
                    .map(|condition| format!("if {condition}"))
                    .collect::<Vec<_>>();
                let fail_conditions = powered
                    .iter()
                    .map(|condition| format!("unless {condition}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                vec![
                    observations,
                    pass_conditions.into_iter().map(|condition| format!("execute {condition} run tellraw @a {{\"text\":\"PASS {name}=1\",\"color\":\"green\"}}")) .collect::<Vec<_>>().join("\n"),
                    format!("execute {fail_conditions} run tellraw @a {{\"text\":\"FAIL {name}: expected powered redstone wire\",\"color\":\"red\"}}"),
                ]
            } else {
                vec![
                    observations,
                    format!("execute if block {location} {off} run tellraw @a {{\"text\":\"PASS {name}=0\",\"color\":\"green\"}}"),
                    format!("execute unless block {location} {off} run tellraw @a {{\"text\":\"FAIL {name}: expected 0\",\"color\":\"red\"}}"),
                ]
            }
        }).collect::<Vec<_>>().join("\n") + "\n";
        pack.insert_text(format!("{root}/_check_{tag_bits}.mcfunction"), checks);
        pack.insert_text(format!("{root}/check_{tag_bits}.mcfunction"), format!("execute at @e[type=minecraft:marker,tag={tag},limit=1] run function {}:{circuit_name}/_check_{tag_bits}\n", config.namespace));
        test_lines.push(format!(
            "schedule function {}:{circuit_name}/stimulate_{tag_bits} {}t replace",
            config.namespace,
            config.settle_ticks + 2 + u32::try_from(bits).expect("case count fits u32") * window
        ));
    }
    test_lines.push(format!(
        "schedule function {}:{circuit_name}/complete {}t replace",
        config.namespace,
        config.settle_ticks + 2 + u32::try_from(cases).expect("case count fits u32") * window
    ));
    pack.insert_text(
        format!("{root}/complete.mcfunction"),
        "tellraw @a {\"text\":\"DUSTROUTE COMPLETE\",\"color\":\"aqua\"}\n",
    );
    pack.insert_text(
        format!("{root}/tests.mcfunction"),
        test_lines.join("\n") + "\n",
    );
    Ok(pack)
}

#[cfg(test)]
mod tests {
    use crate::circuits::half_adder;
    use crate::compiler::{BaselineCompileConfig, BaselineCompiler};

    use super::*;

    #[test]
    fn exports_java_block_states() {
        let config = JavaExportConfig::default();
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        assert_eq!(
            java_block_state(&repeater, &config).unwrap(),
            "minecraft:repeater[delay=2,facing=west,locked=false,powered=false]"
        );
    }

    #[test]
    fn isolated_build_removes_components_before_supports() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let commands = isolated_build_commands(&world, &JavaExportConfig::default()).unwrap();
        let component_clear = commands
            .iter()
            .position(|line| line.ends_with("replace minecraft:redstone_wire"))
            .unwrap();
        let full_clear = commands
            .iter()
            .position(|line| line.ends_with("minecraft:air replace"))
            .unwrap();
        assert!(component_clear < full_clear);
    }

    #[test]
    fn splits_large_fill_regions_below_command_limit() {
        let regions = split_fill_regions(Pos::new(0, 0, 0), Pos::new(200, 20, 20));
        assert!(regions.len() > 1);
        for (low, high) in regions {
            let volume = i64::from(high.x - low.x + 1)
                * i64::from(high.y - low.y + 1)
                * i64::from(high.z - low.z + 1);
            assert!(volume <= 32_768);
        }
    }

    #[test]
    fn rejects_resource_names_that_can_escape_the_pack_path() {
        assert_eq!(
            validate_resource_name("../outside")
                .unwrap_err()
                .to_string(),
            "invalid resource name: ../outside"
        );
    }

    #[test]
    fn creates_complete_half_adder_pack_and_zip() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let config = JavaExportConfig {
            namespace: "ro_half_rust".into(),
            ..JavaExportConfig::default()
        };
        let pack = compiled_circuit_datapack(&compiled, "half", &config).unwrap();
        assert!(pack.files.contains_key("pack.mcmeta"));
        assert!(
            pack.files
                .contains_key("data/ro_half_rust/function/half/tests.mcfunction")
        );
        assert_eq!(
            pack.files
                .get("data/ro_half_rust/function/half/complete.mcfunction")
                .unwrap(),
            b"tellraw @a {\"text\":\"DUSTROUTE COMPLETE\",\"color\":\"aqua\"}\n"
        );
        let directory =
            std::env::temp_dir().join(format!("dustroute-export-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let zip_path = directory.join("half.zip");
        pack.write_zip(&zip_path).unwrap();
        let file = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("pack.mcmeta").is_ok());
        fs::remove_dir_all(directory).unwrap();
    }
}
