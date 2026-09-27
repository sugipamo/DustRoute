//! Explicit authoring of frozen built-in assets. Runtime lookup never calls here.

use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintPortKind, BlueprintRevisionId, ClassificationRevision,
    ClassificationRevisionId, TypeContract, TypeRevision, TypeRevisionId,
};
use dustroute_library::builtin_blueprints::*;
use dustroute_library::{
    ComponentId, LogicalSpec, Port, PortDirection, Provenance, REDSTONE_COMPILER_XOR_ID,
};

use crate::blueprint::blueprint_from_cell;
use crate::cell_generators as recipes;

pub fn generate_builtin_blueprints() -> Result<BlueprintCatalog, String> {
    let mut catalog = BlueprintCatalog::default();
    type Function = fn(bool, bool) -> bool;
    for (id, name, inputs, evaluate) in [
        (
            NOT_CLASSIFICATION_REVISION,
            "NOT",
            &["a"][..],
            (|a, _| !a) as Function,
        ),
        (
            AND_CLASSIFICATION_REVISION,
            "AND",
            &["a", "b"][..],
            (|a, b| a && b) as Function,
        ),
        (
            OR_CLASSIFICATION_REVISION,
            "OR",
            &["a", "b"][..],
            (|a, b| a || b) as Function,
        ),
        (
            NAND_CLASSIFICATION_REVISION,
            "NAND",
            &["a", "b"][..],
            (|a, b| !(a && b)) as Function,
        ),
        (
            XOR_CLASSIFICATION_REVISION,
            "XOR",
            &["a", "b"][..],
            (|a, b| a ^ b) as Function,
        ),
        (
            BUFFER_CLASSIFICATION_REVISION,
            "Buffer",
            &["in"][..],
            (|a, _| a) as Function,
        ),
    ] {
        let ports = inputs
            .iter()
            .map(|name| Port {
                name: (*name).to_owned(),
                direction: PortDirection::Input,
                bit_width: 1,
            })
            .chain([Port {
                name: "out".into(),
                direction: PortDirection::Output,
                bit_width: 1,
            }])
            .collect();
        let truth_table = (0..(1 << inputs.len()))
            .map(|bits| {
                let mut row: Vec<_> = (0..inputs.len())
                    .map(|index| bits & (1 << index) != 0)
                    .collect();
                row.push(evaluate(row[0], row.get(1).copied().unwrap_or(false)));
                row
            })
            .collect();
        catalog
            .insert_classification(ClassificationRevision {
                id: ClassificationRevisionId::new(id).map_err(str::to_owned)?,
                name: name.to_owned(),
                logical_claim: Some(LogicalSpec {
                    ports,
                    truth_table,
                    stateful: false,
                }),
            })
            .map_err(|error| error.to_string())?;
    }
    for (id, name, port_kind) in [
        (
            WIRE_TYPE_REVISION,
            "Redstone wire output",
            BlueprintPortKind::Wire,
        ),
        (
            BLOCK_POWER_TYPE_REVISION,
            "Powered block output",
            BlueprintPortKind::BlockPower,
        ),
    ] {
        catalog
            .insert_type(TypeRevision {
                id: TypeRevisionId::new(id).map_err(str::to_owned)?,
                name: name.into(),
                contract: TypeContract::Signal { port_kind },
            })
            .map_err(|error| error.to_string())?;
    }
    let provenance = Provenance {
        author: "DustRoute".into(),
        source_url: None,
        license: Some("Apache-2.0".into()),
        retrieved_on: None,
    };
    let external = dustroute_library::builtin_catalog()
        .get(&ComponentId::new(REDSTONE_COMPILER_XOR_ID).map_err(str::to_owned)?)
        .expect("external provenance exists")
        .provenance
        .clone();
    for (revision, classification, cell, provenance) in [
        (
            TERMINAL_REVISION,
            BUFFER_CLASSIFICATION_REVISION,
            recipes::terminal_cell("terminal"),
            provenance.clone(),
        ),
        (
            BUFFER_REVISION,
            BUFFER_CLASSIFICATION_REVISION,
            recipes::buffered_boundary_cell("buffer"),
            provenance.clone(),
        ),
        (
            NOT_TOP_REVISION,
            NOT_CLASSIFICATION_REVISION,
            recipes::not_top_cell(),
            provenance.clone(),
        ),
        (
            NOT_SIDE_REVISION,
            NOT_CLASSIFICATION_REVISION,
            recipes::not_cell(),
            provenance.clone(),
        ),
        (
            AND_REVISION,
            AND_CLASSIFICATION_REVISION,
            recipes::and_cell(),
            provenance.clone(),
        ),
        (
            OR_REVISION,
            OR_CLASSIFICATION_REVISION,
            recipes::or_buffered_cell(),
            provenance.clone(),
        ),
        (
            NAND_REVISION,
            NAND_CLASSIFICATION_REVISION,
            recipes::nand_cell(),
            provenance.clone(),
        ),
        (
            XOR_REVISION,
            XOR_CLASSIFICATION_REVISION,
            recipes::compiled_xor_cell()?,
            provenance.clone(),
        ),
        (
            XOR_COMPACT_REVISION,
            XOR_CLASSIFICATION_REVISION,
            recipes::compact_compiled_xor_cell()?,
            provenance,
        ),
        // Retained as a claim with its original provenance. This does not remove
        // the existing catalog's rejected Minecraft evidence or promote it.
        (
            EXTERNAL_XOR_REVISION,
            XOR_CLASSIFICATION_REVISION,
            recipes::external_xor_cell(),
            external,
        ),
    ] {
        catalog
            .insert_revision(blueprint_from_cell(
                BlueprintRevisionId::new(revision).map_err(str::to_owned)?,
                &cell,
                vec![ClassificationRevisionId::new(classification).map_err(str::to_owned)?],
                provenance,
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(catalog)
}
