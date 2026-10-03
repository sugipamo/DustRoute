//! Revision reports and bounded hypothetical-state analysis.
use crate::failure::FailureCause;
use crate::recorded_revision::*;
use dustroute_physical::Pos;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn virtual_analysis_summary(
    scene: &dustroute_physical::PhysicalScene,
    focus: Pos,
    complete: bool,
) -> VirtualAnalysisSummary {
    let hierarchy = dustroute_ir::derive_hierarchy(scene);
    let mixed = dustroute_ir::build_mixed_ir(&hierarchy);
    let diagnostic = dustroute_translate::diagnostic::diagnose_scene(scene, Some(focus), complete);
    let faults = diagnostic
        .diagnosis
        .findings
        .iter()
        .filter(|finding| {
            matches!(&finding.evidence, dustroute_translate::diagnostic::report::FindingEvidence::Connectivity(f) if f.status == dustroute_translate::diagnostic::CircuitDiagnosticStatus::ProbableFault)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut representations = BTreeMap::<RepresentationKind, usize>::new();
    for node in &mixed.nodes {
        let name = match node.kind {
            dustroute_ir::MixedNodeKind::LogicGate { .. } => RepresentationKind::LogicGate,
            dustroute_ir::MixedNodeKind::TimedCell { .. } => RepresentationKind::TimedCell,
            dustroute_ir::MixedNodeKind::PhysicalRegion => RepresentationKind::PhysicalRegion,
            dustroute_ir::MixedNodeKind::Boundary { .. } => RepresentationKind::Boundary,
        };
        *representations.entry(name).or_default() += 1;
    }
    VirtualAnalysisSummary {
        health: diagnostic.health,
        diagnostic_counts: diagnostic.counts,
        source_counts: diagnostic.source_counts,
        probable_faults: faults,
        mixed_ir: MixedSummary {
            physical_components: mixed.physical_component_count,
            recognized_components: mixed.recognized_component_count,
            unresolved_components: mixed.unresolved_component_count,
            nodes: mixed.nodes.len(),
            edges: mixed.edges.len(),
            representations,
        },
        identity: crate::recorded_analysis::circuit_identity(&hierarchy, None, complete, 0),
        scope: "static connectivity and capability diagnostics; no piston flight or harvest contract evaluated".into(),
        functional_behavior_verified: false,
    }
}

pub(in super::super) fn revision_json(
    r: &crate::revision::CircuitRevision,
    include_snapshot: bool,
) -> Value {
    let mut value = json!({"ok":true,"schema_version":r.schema_version,"analysis_mode":"virtual_circuit_revision","revision_id":r.revision_id,"parent_revision_ids":r.parent_revision_ids,"base_observation_id":r.base_observation_id,"dimension":r.dimension,"bounds":{"min":r.snapshot.min,"max":r.snapshot.max},"changes":r.changes,"validation":r.validation,"analysis_complete":r.complete,"mutation_performed":false,"live_world_evidence":false,"placement_authorized":false,"retention":"DUSTROUTE_PLAN_TTL_SECONDS (default 3600 seconds); reads do not extend lifetime"});
    if include_snapshot {
        value["snapshot"] = json!(r.snapshot);
        value["assembly_revision"] = json!(r.assembly);
    }
    value["assembly_state"] = match &r.assembly {
        Some(record) => json!({
            "status": "available",
            "assembly_revision_id": record.id,
            "parent_assembly_revision_ids": record.parents,
            "source_revision_ids": record.assembly.instances.iter().map(|instance| &instance.revision).collect::<std::collections::BTreeSet<_>>(),
            "block_records": record.assembly.blocks.len(),
            "source_instances": record.assembly.instances.len(),
            "connections": record.assembly.connections.len(),
            "scope": "saved hypothetical state; source references are interpretations, not restored evidence"
        }),
        None => json!({"status": "unavailable_or_legacy"}),
    };
    value
}

pub(in super::super) fn revision_validation(
    snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
    dimension: &str,
    focus: Pos,
    complete: bool,
    ticks: usize,
) -> StateValidation {
    let _measurement = crate::performance::span(crate::performance::Phase::StaticValidation);
    let world = match dustroute_translate::snapshot::world_from_snapshot(snapshot) {
        Ok(world) => world,
        Err(error) => {
            return StateValidation::Unavailable {
                error: error.to_string(),
                simulation: SimulationValidation::NotRun { reason: None },
            };
        }
    };
    let issues = world.placement_issues();
    let bounds = dustroute_translate::world_reverse::RegionBounds::new(snapshot.min, snapshot.max);
    let mut analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
    analysis.scene.observation.dimension = dimension.into();
    let summary = virtual_analysis_summary(&analysis.scene, focus, complete);
    let simulation = if !complete || !issues.is_empty() {
        SimulationValidation::NotRun {
            reason: Some("incomplete observation or unsupported/invalid placement".into()),
        }
    } else {
        match simulated_terminal_summary(&world, &analysis, ticks) {
            Ok(result) => SimulationValidation::Simulated { result },
            Err(error) => SimulationValidation::Unavailable { error },
        }
    };
    let valid = issues.is_empty();
    let report = StateReport {
        placement_issues: issues,
        summary,
        simulation,
        property_validation:
            "simulator-supported properties only; not an exhaustive Java block-state schema".into(),
    };
    if !complete {
        StateValidation::Incomplete(report)
    } else if valid {
        StateValidation::StructurallyValid(report)
    } else {
        StateValidation::InvalidOrUnsupported(report)
    }
}

pub(in super::super) fn electrical_modification_validation(
    before: &dustroute_translate::snapshot::MinecraftSnapshot,
    after: &dustroute_translate::snapshot::MinecraftSnapshot,
    complete: bool,
) -> Option<ElectricalValidation> {
    if !before
        .blocks
        .iter()
        .chain(&after.blocks)
        .any(|b| dustroute_translate::world::physical::requires_callback_runtime_name(&b.name))
    {
        return None;
    }
    if !complete {
        return Some(ElectricalValidation::NotRun {
            reason: "incomplete observation".into(),
            changed_positions: None,
            placement_authorized: None,
        });
    }
    if let (Ok(old), Ok(new)) = (
        crate::revision::blocks(before),
        crate::revision::blocks(after),
    ) {
        let changed = old
            .keys()
            .chain(new.keys())
            .copied()
            .filter(|p| old.get(p) != new.get(p))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if changed > 64 {
            return Some(ElectricalValidation::NotRun {
                reason:
                    "large diff requires work_regions and per-region common-runtime verification"
                        .into(),
                changed_positions: Some(changed),
                placement_authorized: Some(false),
            });
        }
    }
    let _measurement = crate::performance::span(crate::performance::Phase::ModelProof);
    Some(
        match dustroute_translate::piston_construction::ElectricalModification::new(
            before,
            after,
            Default::default(),
        ) {
            Ok(proof) => {
                ElectricalValidation::Passed {
                    forward_steps: proof.steps(false).len(),
                    undo_steps: proof.steps(true).len(),
                    execution_profile: dustroute_translate::world::time::piston_runtime::ELECTRICAL_PROFILE.into(),
                    scope: "last revision diff; common electrical command physics; no functional/live-world proof".into(),
                    model_initial_queue: "assumed_empty".into(),
                    runtime_history_reconstructed: false,
                }
            }
            Err(error) => {
                ElectricalValidation::FailedOrUnsupported {
                    error: error.to_string(),
                    cause: FailureCause::from(error),
                    placement_authorized: false,
                }
            }
        },
    )
}

fn simulated_terminal_summary(
    world: &dustroute_translate::world::World,
    analysis: &dustroute_translate::world_reverse::RegionAnalysis,
    ticks: usize,
) -> Result<TerminalSimulation, String> {
    let mut simulator = dustroute_translate::sim::RedstoneTickSimulator::new(world.clone())
        .map_err(|error| error.to_string())?;
    let mut state = simulator.snapshot();
    for _ in 0..ticks {
        state = simulator
            .advance_tick()
            .map_err(|error| error.to_string())?;
    }
    let terminal = |item: &dustroute_translate::world_reverse::InferredTerminal| TerminalSample {
        position: item.anchor,
        powered: state.powered(item.anchor),
        strength: state.strength(item.anchor),
        confidence: item.confidence,
    };
    Ok(TerminalSimulation {
        ticks,
        inputs: analysis.inputs.iter().map(terminal).collect(),
        outputs: analysis.outputs.iter().map(terminal).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};

    fn supported_repeater() -> MinecraftSnapshot {
        MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(3, 3, 3),
            blocks: vec![
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 1, 0),
                    name: "minecraft:repeater".into(),
                    properties: [
                        ("facing", "north"),
                        ("delay", "1"),
                        ("powered", "false"),
                        ("locked", "false"),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
                },
            ],
        }
    }

    fn reopened(validation: &StateValidation) -> StateValidation {
        let bytes =
            dustroute_codec::storage::encode("revision-test.v1", validation, 1 << 20).unwrap();
        let result = dustroute_codec::storage::decode("revision-test.v1", &bytes, 1 << 20).unwrap();
        // Compare the MCP projection; JSON is never the intermediate saved form.
        assert_eq!(
            serde_json::to_value(validation).unwrap(),
            serde_json::to_value(&result).unwrap()
        );
        result
    }

    #[test]
    fn revision_records_keep_support_failure_and_incomplete_simulation_distinct() {
        let mut snapshot = supported_repeater();
        let validate = |snapshot: &MinecraftSnapshot, complete| {
            reopened(&revision_validation(
                snapshot,
                "minecraft:overworld",
                Pos::new(0, 1, 0),
                complete,
                2,
            ))
        };
        let StateValidation::StructurallyValid(report) = validate(&snapshot, true) else {
            panic!("supported repeater must remain structurally valid");
        };
        assert!(report.placement_issues.is_empty());
        assert!(
            matches!(report.simulation, SimulationValidation::Simulated { result } if result.ticks == 2)
        );
        assert!(!report.summary.functional_behavior_verified);

        let StateValidation::Incomplete(report) = validate(&snapshot, false) else {
            panic!("incomplete observation must remain incomplete");
        };
        assert!(matches!(
            report.simulation,
            SimulationValidation::NotRun { reason: Some(_) }
        ));
        snapshot.blocks.remove(0);
        let StateValidation::InvalidOrUnsupported(report) = validate(&snapshot, true) else {
            panic!("missing support must remain an invalid placement");
        };
        assert!(!report.placement_issues.is_empty());
        assert!(matches!(
            report.simulation,
            SimulationValidation::NotRun { .. }
        ));
        snapshot.blocks[0]
            .properties
            .insert("facing".into(), "invalid".into());
        assert!(matches!(
            validate(&snapshot, true),
            StateValidation::Unavailable {
                simulation: SimulationValidation::NotRun { reason: None },
                ..
            }
        ));
    }

    #[test]
    fn revision_records_keep_large_electrical_diff_refusal_after_reopen() {
        let mut before = supported_repeater();
        before.max = Pos::new(70, 3, 3);
        before.blocks.push(MinecraftSnapshotBlock {
            pos: Pos::new(3, 1, 2),
            name: "minecraft:piston".into(),
            properties: [
                ("facing".into(), "east".into()),
                ("extended".into(), "false".into()),
            ]
            .into(),
        });
        let mut after = before.clone();
        after
            .blocks
            .extend((1..=65).map(|x| MinecraftSnapshotBlock {
                pos: Pos::new(x, 0, 0),
                name: "minecraft:stone".into(),
                properties: BTreeMap::new(),
            }));
        let validation = electrical_modification_validation(&before, &after, true).unwrap();
        let bytes =
            dustroute_codec::storage::encode("electrical-test.v1", &validation, 65536).unwrap();
        let reopened: ElectricalValidation =
            dustroute_codec::storage::decode("electrical-test.v1", &bytes, 65536).unwrap();
        assert!(matches!(
            reopened,
            ElectricalValidation::NotRun {
                changed_positions: Some(65),
                placement_authorized: Some(false),
                ..
            }
        ));
    }
}
