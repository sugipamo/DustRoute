//! Independent device realizations and physical connection requirements.
//! The lever owns one block; its attachment support belongs to its environment
//! or is shared with another realization. Placement still needs fresh review.
use crate::blueprint::BlueprintCatalog;
use std::sync::OnceLock;
mod data;

pub const LEVER_TYPE_REVISION: &str = "dustroute.type.lever.v1";
pub const DEVICE_OUTPUT_TYPE_REVISION: &str = "dustroute.type.device-output.v1";
pub const LEGACY_LEVER_REVISION: &str = "dustroute.lever.wall.v1";
/// Current realization; older pinned sources remain unchanged in the catalog.
pub const LEVER_REVISION: &str = "dustroute.lever.wall.v2";

pub fn builtin_primitives() -> &'static BlueprintCatalog {
    static CATALOG: OnceLock<BlueprintCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        data::records()
            .catalog()
            .expect("fixed primitive definitions are structurally valid")
    })
}
