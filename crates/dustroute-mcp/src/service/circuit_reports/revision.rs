//! Revision reports and bounded hypothetical-state analysis.
use super::analysis::circuit_identity_json;
use dustroute_physical::Pos;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn virtual_analysis_summary(
    scene: &dustroute_physical::PhysicalScene,
    focus: Pos,
    complete: bool,
) -> Value {
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
        .collect::<Vec<_>>();
    let mut representations = BTreeMap::<&str, usize>::new();
    for node in &mixed.nodes {
        let name = match node.kind {
            dustroute_ir::MixedNodeKind::LogicGate { .. } => "logic_gate",
            dustroute_ir::MixedNodeKind::TimedCell { .. } => "timed_cell",
            dustroute_ir::MixedNodeKind::PhysicalRegion => "physical_region",
            dustroute_ir::MixedNodeKind::Boundary { .. } => "boundary",
        };
        *representations.entry(name).or_default() += 1;
    }
    json!({
        "health": diagnostic.health,
        "diagnostic_counts": diagnostic.counts,
        "source_counts": diagnostic.source_counts,
        "probable_faults": faults,
        "mixed_ir": {
            "physical_components": mixed.physical_component_count,
            "recognized_components": mixed.recognized_component_count,
            "unresolved_components": mixed.unresolved_component_count,
            "nodes": mixed.nodes.len(),
            "edges": mixed.edges.len(),
            "representations": representations,
        },
        "identity": circuit_identity_json(&hierarchy, None, complete, 0),
        "scope": "static connectivity and capability diagnostics; no piston flight or harvest contract evaluated",
        "functional_behavior_verified": false,
    })
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
) -> Value {
    let world = match dustroute_translate::snapshot::world_from_snapshot(snapshot) {
        Ok(world) => world,
        Err(error) => {
            return json!({"status":"unavailable","error":error.to_string(),"simulation":{"status":"not_run"}});
        }
    };
    let issues = world.placement_issues();
    let bounds = dustroute_translate::world_reverse::RegionBounds::new(snapshot.min, snapshot.max);
    let mut analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
    analysis.scene.observation.dimension = dimension.into();
    let summary = virtual_analysis_summary(&analysis.scene, focus, complete);
    let simulation = if !complete || !issues.is_empty() {
        json!({"status":"not_run","reason":"incomplete observation or unsupported/invalid placement"})
    } else {
        match simulated_terminal_summary(&world, &analysis, ticks) {
            Ok(result) => json!({"status":"simulated","result":result}),
            Err(error) => json!({"status":"unavailable","error":error}),
        }
    };
    json!({"status":if !complete {"incomplete"} else if issues.is_empty() {"structurally_valid"} else {"invalid_or_unsupported"},"placement_issues":issues,"summary":summary,"simulation":simulation,"property_validation":"simulator-supported properties only; not an exhaustive Java block-state schema"})
}

pub(in super::super) fn electrical_modification_validation(
    before: &dustroute_translate::snapshot::MinecraftSnapshot,
    after: &dustroute_translate::snapshot::MinecraftSnapshot,
    complete: bool,
) -> Option<Value> {
    if !before
        .blocks
        .iter()
        .chain(&after.blocks)
        .any(|b| dustroute_translate::world::physical::requires_callback_runtime_name(&b.name))
    {
        return None;
    }
    if !complete {
        return Some(json!({"status":"not_run","reason":"incomplete observation"}));
    }
    Some(
        match dustroute_translate::piston_construction::ElectricalModification::new(
            before,
            after,
            Default::default(),
        ) {
            Ok(proof) => {
                json!({"status":"passed","forward_steps":proof.steps(false).len(),"undo_steps":proof.steps(true).len(),
            "execution_profile":dustroute_translate::world::time::piston_runtime::ELECTRICAL_PROFILE,
            "scope":"last revision diff; common electrical command physics; no functional/live-world proof",
            "model_initial_queue":"assumed_empty","runtime_history_reconstructed":false})
            }
            Err(error) => {
                json!({"status":"failed_or_unsupported","error":error,"placement_authorized":false})
            }
        },
    )
}

fn simulated_terminal_summary(
    world: &dustroute_translate::world::World,
    analysis: &dustroute_translate::world_reverse::RegionAnalysis,
    ticks: usize,
) -> Result<Value, String> {
    let mut simulator = dustroute_translate::sim::RedstoneTickSimulator::new(world.clone())
        .map_err(|error| error.to_string())?;
    let mut state = simulator.snapshot();
    for _ in 0..ticks {
        state = simulator
            .advance_tick()
            .map_err(|error| error.to_string())?;
    }
    let terminal = |item: &dustroute_translate::world_reverse::InferredTerminal| {
        json!({
            "position": item.anchor,
            "powered": state.powered(item.anchor),
            "strength": state.strength(item.anchor),
            "confidence": item.confidence,
        })
    };
    Ok(json!({
        "ticks": ticks,
        "inputs": analysis.inputs.iter().map(terminal).collect::<Vec<_>>(),
        "outputs": analysis.outputs.iter().map(terminal).collect::<Vec<_>>(),
    }))
}
