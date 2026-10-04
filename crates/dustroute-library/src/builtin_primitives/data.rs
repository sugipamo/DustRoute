//! Fixed records transcribed from the independent historical fixture.
//! No recipe compiler or presentation decoder runs while loading this table.

use super::{
    DEVICE_OUTPUT_TYPE_REVISION, LEGACY_LEVER_REVISION, LEVER_REVISION, LEVER_TYPE_REVISION,
};

use crate::blueprint::{
    BlueprintPort, BlueprintPortKind, BlueprintRecords, BlueprintRevision, BlueprintRevisionId,
    StaticTypeBinding, TypeContract, TypeRevision, TypeRevisionId,
};

use crate::{PortDirection, Provenance};

use crate::builtin_definitions::{Cell, cell};

use dustroute_minecraft::{BlockKind, Pos};

const LEGACY_LEVER_CELLS: &[Cell] = &[cell(Pos::new(0, 0, 0), BlockKind::Lever)
    .powered(false)
    .support(Pos::new(1, 0, 0))];

const LEVER_CELLS: &[Cell] = &[cell(Pos::new(0, 0, 0), BlockKind::Lever)
    .powered(false)
    .support(Pos::new(1, 0, 0))];

pub(super) fn records() -> BlueprintRecords {
    BlueprintRecords {
        types: vec![
            TypeRevision {
                id: TypeRevisionId::new(DEVICE_OUTPUT_TYPE_REVISION)
                    .expect("fixed built-in identifier"),
                name: "Direct device output".into(),
                contract: TypeContract::Signal {
                    port_kind: BlueprintPortKind::DeviceOutput,
                },
            },
            TypeRevision {
                id: TypeRevisionId::new(LEVER_TYPE_REVISION).expect("fixed built-in identifier"),
                name: "Lever block identity".into(),
                contract: TypeContract::BlockKind {
                    block_kind: BlockKind::Lever,
                },
            },
        ],
        classifications: vec![],
        revisions: vec![
            BlueprintRevision {
                id: BlueprintRevisionId::new(LEGACY_LEVER_REVISION)
                    .expect("fixed built-in identifier"),
                parents: vec![],
                name: "Independent wall lever".into(),
                classifications: vec![],
                blocks: LEGACY_LEVER_CELLS
                    .iter()
                    .copied()
                    .map(Cell::positioned)
                    .collect(),
                ports: vec![BlueprintPort {
                    name: "out".into(),
                    direction: PortDirection::Output,
                    position: Pos::new(0, 0, 0),
                    kind: BlueprintPortKind::DeviceOutput,
                    facing: None,
                    required_source_types: vec![],
                }],
                static_type_bindings: vec![],
                provenance: Provenance {
                    author: "DustRoute".into(),
                    source_url: None,
                    license: Some("Apache-2.0".into()),
                    retrieved_on: None,
                },
                inclusions: vec![],
                connections: vec![],
                port_bindings: vec![],
                behavior_bindings: vec![],
                required_laws: vec![],
                initial_layout: None,
                law: None,
            },
            BlueprintRevision {
                id: BlueprintRevisionId::new(LEVER_REVISION).expect("fixed built-in identifier"),
                parents: vec![
                    BlueprintRevisionId::new(LEGACY_LEVER_REVISION)
                        .expect("fixed built-in identifier"),
                ],
                name: "Independent wall lever".into(),
                classifications: vec![],
                blocks: LEVER_CELLS.iter().copied().map(Cell::positioned).collect(),
                ports: vec![BlueprintPort {
                    name: "out".into(),
                    direction: PortDirection::Output,
                    position: Pos::new(0, 0, 0),
                    kind: BlueprintPortKind::DeviceOutput,
                    facing: None,
                    required_source_types: vec![],
                }],
                static_type_bindings: vec![StaticTypeBinding {
                    type_revision: TypeRevisionId::new(LEVER_TYPE_REVISION)
                        .expect("fixed built-in identifier"),
                    port: "out".into(),
                }],
                provenance: Provenance {
                    author: "DustRoute".into(),
                    source_url: None,
                    license: Some("Apache-2.0".into()),
                    retrieved_on: None,
                },
                inclusions: vec![],
                connections: vec![],
                port_bindings: vec![],
                behavior_bindings: vec![],
                required_laws: vec![],
                initial_layout: None,
                law: None,
            },
        ],
        assemblies: vec![],
    }
}
