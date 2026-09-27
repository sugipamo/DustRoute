//! Frozen built-in realizations. Loading this catalog cannot invoke a compiler.

use std::sync::OnceLock;

use crate::blueprint::BlueprintCatalog;

pub const NOT_CLASSIFICATION_REVISION: &str = "dustroute.classification.not.v1";
pub const AND_CLASSIFICATION_REVISION: &str = "dustroute.classification.and.v1";
pub const OR_CLASSIFICATION_REVISION: &str = "dustroute.classification.or.v1";
pub const NAND_CLASSIFICATION_REVISION: &str = "dustroute.classification.nand.v1";
pub const XOR_CLASSIFICATION_REVISION: &str = "dustroute.classification.xor.v1";
pub const BUFFER_CLASSIFICATION_REVISION: &str = "dustroute.classification.buffer.v1";

pub const WIRE_TYPE_REVISION: &str = "dustroute.type.wire-output.v1";
pub const BLOCK_POWER_TYPE_REVISION: &str = "dustroute.type.block-power-output.v1";

pub const TERMINAL_REVISION: &str = "dustroute.terminal.v1";
pub const BUFFER_REVISION: &str = "dustroute.buffer.v1";
pub const NOT_TOP_REVISION: &str = "dustroute.not.torch-top.v1";
pub const NOT_SIDE_REVISION: &str = "dustroute.not.torch-side.v1";
pub const AND_REVISION: &str = "dustroute.and.demorgan.v1";
pub const OR_REVISION: &str = "dustroute.or.buffered.v1";
pub const NAND_REVISION: &str = "dustroute.nand.merge.v1";
pub const XOR_REVISION: &str = "dustroute.xor.compiled.v1";
pub const XOR_COMPACT_REVISION: &str = "dustroute.xor.compact.v1";
pub const EXTERNAL_XOR_REVISION: &str = "redstone-compiler.xor-generated.v1";

pub const BUILTIN_BLUEPRINT_ARCHIVE: &str = include_str!("../blueprints/builtin-v1.json");

#[must_use]
pub fn builtin_blueprints() -> &'static BlueprintCatalog {
    static CATALOG: OnceLock<BlueprintCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        BlueprintCatalog::from_json(BUILTIN_BLUEPRINT_ARCHIVE)
            .expect("embedded built-in blueprint archive is valid")
    })
}
