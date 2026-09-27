use dustroute_library::PortDirection;
use dustroute_library::assembly::*;
use dustroute_library::behavior_type::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::{
    DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws, torch_law_revision,
};
use dustroute_optimize::blueprint_reduction::*;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::{
    BlueprintUpdateRequest, BlueprintUpdates, UpdateStatus,
};
use dustroute_translate::promotion::{CheckStatus, review_assembly_in_context};
use dustroute_translate::{Block, BlockKind, Facing, Pos, Region, World};
use std::collections::BTreeMap;

fn id(s: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(s).unwrap()
}
fn state_id(s: &str) -> AssemblyRevisionId {
    AssemblyRevisionId::new(s).unwrap()
}
fn path(s: &str) -> InstancePath {
    vec![InstanceId::new(s).unwrap()]
}
fn include(name: &str, revision: &str) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision: id(revision),
        origin: Pos::default(),
        rotation: Default::default(),
    }
}
fn port(name: &str, pos: Pos, direction: PortDirection) -> BlueprintPort {
    BlueprintPort {
        name: name.into(),
        position: pos,
        direction,
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: vec![],
    }
}
fn fixture() -> (BlueprintCatalog, BlueprintReductionRequest) {
    let mut catalog = builtin_laws().clone();
    let type_id = TypeRevisionId::new("test.repeated-not.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: type_id.clone(),
            name: "Explicit relation".into(),
            contract: TypeContract::RepeatedSettling {
                relation: RepeatedSettling {
                    inputs: vec!["a".into()],
                    outputs: vec!["out".into()],
                    rows: [false, true]
                        .into_iter()
                        .map(|b| BooleanRow {
                            inputs: vec![b],
                            outputs: vec![!b],
                        })
                        .collect(),
                },
            },
        })
        .unwrap();
    let binding = BehaviorBinding::RepeatedSettling {
        behavior_type: type_id,
        inputs: BTreeMap::from([("a".into(), "input".into())]),
        outputs: BTreeMap::from([("out".into(), "output".into())]),
    };
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(-2, -1, 0), Block::new(BlockKind::Solid));
    let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
    lever.support_offset = Some(Pos::new(1, 0, 0));
    lever.powered = Some(false);
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    torch.facing = Some(Facing::East);
    world.place(BlockKind::RedstoneWire, Pos::new(-2, 0, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
    let blocks: Vec<_> = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    let source = BlueprintRevision {
        id: id("original.v1"),
        parents: vec![],
        name: "NOT with removable terminal stubs".into(),
        classifications: vec![],
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![binding.clone()],
        blocks: blocks.clone(),
        initial_layout: None,
        law: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        ports: vec![
            port("input", Pos::new(-2, 0, 0), PortDirection::Input),
            port("output", Pos::new(1, 2, 0), PortDirection::Output),
        ],
        provenance: torch_law_revision().provenance.clone(),
    };
    catalog.insert_revision(source.clone()).unwrap();
    let mut parent = source.clone();
    parent.id = id("parent.v1");
    parent.blocks.clear();
    parent.inclusions = vec![
        include("child", "original.v1"),
        include("shared", "original.v1"),
    ];
    parent.behavior_bindings.clear();
    parent.ports.clear();
    catalog.insert_revision(parent).unwrap();
    let base = AssemblyRevision {
        id: state_id("base.v1"),
        parents: vec![],
        assembly: Assembly {
            name: "Shared physical state".into(),
            instances: vec![include("root", "parent.v1")],
            blocks,
            known_regions: vec![Region::new(Pos::new(-5, -5, -5), Pos::new(8, 8, 8))],
            connections: vec![],
            boundaries: vec![],
        },
    };
    catalog.insert_assembly(base).unwrap();
    let context = PhysicalBehaviorContext {
        profile: PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        dust_law: id(DUST_LAW_REVISION),
        torch_law: id(TORCH_LAW_REVISION),
        max_electrical_iterations: 128,
        input_drivers: vec![PhysicalInputDriver {
            port_position: Pos::new(-2, 0, 0),
            port_kind: BlueprintPortKind::Wire,
            lever_position: Pos::new(-1, 0, 0),
        }],
    };
    let request = BlueprintReductionRequest {
        scope: Default::default(),
        base_state: state_id("base.v1"),
        target_instance: vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("child").unwrap(),
        ],
        target: binding,
        candidate_revision: id("reduced.v1"),
        candidate_state: state_id("reduced.state.v1"),
        behavior_context: context,
        alternatives: vec![],
    };
    (catalog, request)
}

#[test]
fn search_reduces_real_blocks_moves_both_ports_and_does_not_rewrite_old_interpretations() {
    let (catalog, request) = fixture();
    let before = catalog.to_json().unwrap();
    let result =
        reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default()).unwrap();
    eprintln!(
        "stats={:?}; stop={}; baseline={:?}",
        result.stats, result.stop_reason, result.baseline.behavior
    );
    let best = result
        .best
        .as_ref()
        .expect("a smaller typed NOT must be found");
    assert_eq!(result.baseline_blocks, 7);
    assert_eq!(best.occupied_blocks, 3);
    assert!(!result.global_minimality_proven);
    assert_eq!(result.prior_interpretations.len(), 3);
    assert!(best.candidate.blueprint.inclusions.is_empty());
    assert_eq!(
        best.candidate.blueprint.required_laws,
        request
            .behavior_context
            .law_revisions()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    );
    let input = best
        .candidate
        .blueprint
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap();
    let output = best
        .candidate
        .blueprint
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .unwrap();
    assert_eq!(input.position, Pos::default());
    assert_eq!(output.position, Pos::new(1, 0, 0));
    assert_eq!(input.kind, BlueprintPortKind::BlockPower);
    assert_eq!(output.kind, BlueprintPortKind::DeviceOutput);
    assert_eq!(
        best.candidate.behavior_context.torch_law,
        request.behavior_context.torch_law
    );
    assert!(
        best.review
            .behavior
            .iter()
            .all(|c| c.status == CheckStatus::Passed)
    );
    assert_eq!(catalog.to_json().unwrap(), before);
    let mut saved = catalog.clone();
    saved
        .insert_revision(best.candidate.blueprint.clone())
        .unwrap();
    saved.insert_assembly(best.candidate.state.clone()).unwrap();
    let saved = BlueprintCatalog::from_json(&saved.to_json().unwrap()).unwrap();
    let fresh = review_assembly_in_context(
        &saved,
        &best.candidate.state.assembly,
        Some(&best.candidate.behavior_context),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(fresh.status(), CheckStatus::Passed);
}

#[test]
fn changed_laws_unknown_context_and_search_limits_do_not_turn_partial_results_into_proof() {
    let (catalog, mut request) = fixture();
    let exhausted = reduce_blueprint_blocks(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_layouts: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(exhausted.best.is_none());
    assert!(exhausted.stop_reason.contains("budget exhausted"));
    assert!(!exhausted.global_minimality_proven);
    let zero = reduce_blueprint_blocks(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_millis: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(zero.best.is_none());
    assert!(
        zero.baseline
            .behavior
            .iter()
            .any(|c| c.status == CheckStatus::Undetermined)
    );
    request.behavior_context.profile = PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1;
    let unknown =
        reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default()).unwrap();
    assert!(unknown.best.is_none());
    assert_eq!(unknown.stats.layouts_examined, 0);
}

#[test]
fn explicit_arbitrary_rewrite_can_move_controls_but_cannot_change_laws_or_hide_retained_children() {
    let (mut catalog, mut request) = fixture();
    let generated =
        reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default())
            .unwrap()
            .best
            .unwrap();
    let mut relocated = generated.candidate;
    let mut borrowed = relocated.blueprint.clone();
    borrowed.id = id("borrowed.compact.v1");
    borrowed.parents.clear();
    catalog.insert_revision(borrowed.clone()).unwrap();
    let delta = Pos::new(2, 2, 2);
    relocated.blueprint.inclusions = vec![BlueprintInclusion {
        origin: delta,
        ..include("borrowed", borrowed.id.as_str())
    }];
    let shift = |p: Pos| Pos::new(p.x + delta.x, p.y + delta.y, p.z + delta.z);
    for b in &mut relocated.state.assembly.blocks {
        b.position = shift(b.position);
    }
    for p in &mut relocated.blueprint.ports {
        p.position = shift(p.position);
    }
    relocated.blueprint.initial_layout.as_mut().unwrap().blocks =
        relocated.state.assembly.blocks.clone();
    for d in &mut relocated.behavior_context.input_drivers {
        d.port_position = shift(d.port_position);
        d.lever_position = shift(d.lever_position);
    }
    request.alternatives = vec![relocated.clone()];
    let compared = reduce_blueprint_blocks(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_layouts: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        compared.best.as_ref().unwrap().candidate.behavior_context,
        relocated.behavior_context
    );
    assert_eq!(compared.best.as_ref().unwrap().occupied_blocks, 3);
    assert_eq!(compared.best.as_ref().unwrap().review.behavior.len(), 2);
    assert_eq!(catalog.revision(&borrowed.id), Some(&borrowed));
    request.alternatives[0].behavior_context.torch_law = id(DUST_LAW_REVISION);
    assert!(
        reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default()).is_err()
    );
    // Keeping a child whose ports have been removed still requires its checks.
    relocated.blueprint.inclusions = vec![include("retained", "original.v1")];
    request.alternatives = vec![relocated];
    let rejected = reduce_blueprint_blocks(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_layouts: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(rejected.best.is_none());
    assert_eq!(rejected.stats.failed, 1);
}

#[test]
fn reduced_candidate_enters_existing_parent_proposal_and_is_reverified_after_reload() {
    let (catalog, request) = fixture();
    let best = reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default())
        .unwrap()
        .best
        .unwrap();
    let mut parent = catalog.revision(&id("parent.v1")).unwrap().clone();
    parent.id = id("parent.v2");
    parent.parents = vec![id("parent.v1")];
    parent.inclusions = vec![include("child", "reduced.v1")];
    let mut candidate = best.candidate.state.clone();
    candidate.assembly.instances = vec![include("root", "parent.v2")];
    for boundary in &mut candidate.assembly.boundaries {
        boundary.port.instance = vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("child").unwrap(),
        ];
    }
    let mut updates = BlueprintUpdates::new(catalog.clone());
    let proposal = BlueprintUpdateId::new("reduction.proposal.v1").unwrap();
    updates
        .create(BlueprintUpdateRequest {
            id: proposal.clone(),
            title: "Reduce by target type".into(),
            description: "Explicitly replace prior shared decomposition".into(),
            base_state: request.base_state,
            base_parent: id("parent.v1"),
            candidate_parent: parent.id.clone(),
            parent_instance: path("root"),
            child_before: path("child"),
            previous_child: id("original.v1"),
            child_after: path("child"),
            next_child: best.candidate.blueprint.id.clone(),
            revisions: vec![parent, best.candidate.blueprint],
            candidate_state: candidate,
            behavior_context: Some(best.candidate.behavior_context.into()),
        })
        .unwrap();
    assert_eq!(
        updates.validate(&proposal).unwrap().status(),
        CheckStatus::Passed
    );
    let mut loaded = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    loaded.adopt(&proposal).unwrap();
    assert_eq!(
        loaded.proposal(&proposal).unwrap().status(),
        UpdateStatus::Adopted
    );
    assert_eq!(
        loaded.catalog().revision(&id("parent.v1")),
        catalog.revision(&id("parent.v1"))
    );
    assert_eq!(
        loaded.catalog().revision(&id("original.v1")),
        catalog.revision(&id("original.v1"))
    );
}

#[test]
fn existing_builtin_not_realizations_reduce_by_observing_the_torch_directly() {
    use dustroute_library::builtin_blueprints::{
        NOT_SIDE_REVISION, NOT_TOP_REVISION, builtin_blueprints,
    };
    for builtin in [NOT_TOP_REVISION, NOT_SIDE_REVISION] {
        let (definitions, mut request) = fixture();
        let mut catalog = builtin_blueprints().clone();
        for law in builtin_laws().revisions() {
            catalog.insert_revision(law.clone()).unwrap();
        }
        catalog
            .insert_type(
                definitions
                    .type_revision(request.target.behavior_type())
                    .unwrap()
                    .clone(),
            )
            .unwrap();
        let source = catalog.revision(&id(builtin)).unwrap().clone();
        let mut blocks = source.blocks.clone();
        let mut world = World::new();
        world.set(Pos::default(), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.support_offset = Some(Pos::new(1, 0, 0));
        lever.powered = Some(false);
        blocks.push(PositionedBlock {
            position: Pos::new(-1, 0, 0),
            block: lever.clone(),
        });
        catalog
            .insert_assembly(AssemblyRevision {
                id: request.base_state.clone(),
                parents: vec![],
                assembly: Assembly {
                    name: builtin.into(),
                    instances: vec![include("gate", builtin)],
                    blocks,
                    known_regions: vec![Region::new(Pos::new(-5, -5, -5), Pos::new(8, 8, 8))],
                    connections: vec![],
                    boundaries: vec![],
                },
            })
            .unwrap();
        request.target_instance = path("gate");
        request.target = BehaviorBinding::RepeatedSettling {
            behavior_type: request.target.behavior_type().clone(),
            inputs: BTreeMap::from([("a".into(), "a".into())]),
            outputs: BTreeMap::from([("out".into(), "out".into())]),
        };
        request.behavior_context.input_drivers[0].port_position = Pos::default();
        request.behavior_context.input_drivers[0].port_kind = BlueprintPortKind::BlockPower;
        let before = catalog.to_json().unwrap();
        let result =
            reduce_blueprint_blocks(&catalog, &request, BlueprintReductionBudget::default())
                .unwrap();
        eprintln!("{builtin}: {:?}; {}", result.stats, result.stop_reason);
        let best = result
            .best
            .expect("existing NOT should improve, without a hand-authored replacement");
        assert_eq!(result.baseline_blocks, 5);
        assert_eq!(best.occupied_blocks, 3);
        let output = best
            .candidate
            .blueprint
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Output)
            .unwrap();
        assert_eq!(output.kind, BlueprintPortKind::DeviceOutput);
        assert_ne!(
            output.position,
            source
                .ports
                .iter()
                .find(|p| p.direction == PortDirection::Output)
                .unwrap()
                .position
        );
        assert_eq!(catalog.to_json().unwrap(), before);
    }
}

#[test]
fn component_search_separates_fixed_equipment_and_counts_shared_support_once() {
    let (mut catalog, mut request) = fixture();
    let base = catalog.assembly(&request.base_state).unwrap().clone();
    let lever = Pos::new(-1, 0, 0);
    let mut equipment = catalog.revision(&id("original.v1")).unwrap().clone();
    equipment.id = id("external-control.v1");
    equipment.name = "External lever and shared support".into();
    equipment.behavior_bindings.clear();
    equipment.ports = vec![];
    equipment
        .blocks
        .retain(|b| b.position == lever || b.position == Pos::default());
    catalog.insert_revision(equipment).unwrap();
    request.scope = BlueprintReductionScope::Component {
        body_positions: base
            .assembly
            .blocks
            .iter()
            .filter(|b| b.block.kind != BlockKind::Air && b.position != lever)
            .map(|b| b.position)
            .collect(),
        environment_instances: vec![include("control", "external-control.v1")],
    };
    let before = catalog.to_json().unwrap();
    let result = reduce_blueprint_blocks(&catalog, &request, Default::default()).unwrap();
    assert_eq!(
        (result.baseline_blocks, result.baseline_total_blocks),
        (6, 7)
    );
    let best = result.best.unwrap();
    assert_eq!((best.occupied_blocks, best.total_occupied_blocks), (2, 3));
    assert_eq!(
        best.candidate.state.assembly.instances[1],
        include("control", "external-control.v1")
    );
    assert!(
        best.candidate
            .blueprint
            .initial_layout
            .as_ref()
            .unwrap()
            .blocks
            .iter()
            .all(|b| b.position != lever)
    );
    let actual_lever = best
        .candidate
        .state
        .assembly
        .blocks
        .iter()
        .find(|b| b.position == lever)
        .unwrap();
    assert_eq!(
        Some(actual_lever),
        base.assembly.blocks.iter().find(|b| b.position == lever)
    );
    assert_eq!(catalog.to_json().unwrap(), before);
    // The shared support belongs to the body and the external interpretation,
    // but occupies only one cell in the complete physical Assembly.
    assert!(
        best.candidate
            .blueprint
            .initial_layout
            .unwrap()
            .blocks
            .iter()
            .any(|b| b.position == Pos::default() && b.block.kind == BlockKind::Solid)
    );
}

#[test]
fn component_scope_cannot_hide_controls_or_modify_fixed_equipment() {
    let (catalog, mut request) = fixture();
    let base = catalog.assembly(&request.base_state).unwrap();
    request.scope = BlueprintReductionScope::Component {
        body_positions: base.assembly.blocks.iter().map(|b| b.position).collect(),
        environment_instances: vec![],
    };
    assert!(
        reduce_blueprint_blocks(&catalog, &request, Default::default())
            .unwrap_err()
            .contains("external equipment")
    );
    if let BlueprintReductionScope::Component { body_positions, .. } = &mut request.scope {
        body_positions.retain(|p| *p != Pos::new(-1, 0, 0));
    }
    let result = reduce_blueprint_blocks(&catalog, &request, Default::default()).unwrap();
    let original = result.best.unwrap().candidate;
    let mut uncounted = original.clone();
    uncounted
        .blueprint
        .initial_layout
        .as_mut()
        .unwrap()
        .blocks
        .retain(|b| b.block.kind != BlockKind::RedstoneTorch);
    request.alternatives = vec![uncounted];
    let limited = BlueprintReductionBudget {
        max_layouts: 0,
        ..Default::default()
    };
    assert!(
        reduce_blueprint_blocks(&catalog, &request, limited)
            .unwrap_err()
            .contains("every new occupied cell")
    );
    let mut invented_air = original.clone();
    invented_air.state.assembly.blocks.push(PositionedBlock {
        position: Pos::new(500, 500, 500),
        block: Block::new(BlockKind::Air),
    });
    request.alternatives = vec![invented_air];
    assert!(
        reduce_blueprint_blocks(&catalog, &request, limited)
            .unwrap_err()
            .contains("unknown space")
    );
    let mut candidate = original;
    candidate
        .state
        .assembly
        .blocks
        .iter_mut()
        .find(|b| b.block.kind == BlockKind::Lever)
        .unwrap()
        .block
        .powered = Some(true);
    request.alternatives = vec![candidate];
    let error = reduce_blueprint_blocks(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_layouts: 0,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error.contains("external equipment"), "{error}");
}

#[test]
fn torch_support_patterns_are_enumerated_then_proved_in_all_five_orientations() {
    let (mut catalog, mut search) = fixture();
    let mut base = catalog.assembly(&search.base_state).unwrap().clone();
    base.parents = vec![base.id.clone()];
    base.id = state_id("ceiling-control.state.v1");
    let control = base
        .assembly
        .blocks
        .iter_mut()
        .find(|b| b.block.kind == BlockKind::Lever)
        .unwrap();
    control.position = Pos::new(0, -1, 0);
    control.block.support_offset = Some(Pos::new(0, 1, 0));
    search.behavior_context.input_drivers[0].lever_position = control.position;
    search.scope = BlueprintReductionScope::Component {
        body_positions: base
            .assembly
            .blocks
            .iter()
            .filter(|b| b.block.kind != BlockKind::Lever)
            .map(|b| b.position)
            .collect(),
        environment_instances: vec![],
    };
    search.base_state = base.id.clone();
    catalog.insert_assembly(base).unwrap();
    let before = catalog.to_json().unwrap();
    let request = TorchSupportEnumerationRequest {
        search,
        support_position: Pos::default(),
    };
    // This verifies all five placements, not the production 30-second deadline.
    // Give each bounded proof its normal budget even in a busy workspace run.
    let report = enumerate_torch_supports(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_millis: 180_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.family_exhausted, "{report:?}");
    assert_eq!(report.candidates.len(), 5);
    assert!(!report.global_minimality_proven);
    let mut ids = std::collections::BTreeSet::new();
    for entry in &report.candidates {
        assert_eq!(entry.status, CheckStatus::Passed, "{entry:?}");
        let reviewed = entry.candidate.as_ref().unwrap();
        assert_eq!(
            (reviewed.occupied_blocks, reviewed.total_occupied_blocks),
            (2, 3)
        );
        assert!(ids.insert(reviewed.candidate.blueprint.id.clone()));
        assert_eq!(
            reviewed.candidate.blueprint.ports[1].kind,
            BlueprintPortKind::DeviceOutput
        );
        assert!(
            reviewed
                .review
                .behavior
                .iter()
                .any(|c| c.detail.contains("closed overapproximate graph"))
        );
        assert!(
            reviewed
                .candidate
                .blueprint
                .initial_layout
                .as_ref()
                .unwrap()
                .blocks
                .iter()
                .all(|b| b.block.kind != BlockKind::Lever)
        );
    }
    assert_eq!(catalog.to_json().unwrap(), before);
    let bounded = enumerate_torch_supports(
        &catalog,
        &request,
        BlueprintReductionBudget {
            max_layouts: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!bounded.family_exhausted);
    assert!(bounded.candidates.is_empty());
    // An incompatible target relation is not granted just because the physical
    // family is familiar or a candidate looks like an inverter.
    let mut identity = catalog
        .type_revision(request.search.target.behavior_type())
        .unwrap()
        .clone();
    identity.id = TypeRevisionId::new("test.identity.v1").unwrap();
    if let TypeContract::RepeatedSettling { relation } = &mut identity.contract {
        for row in &mut relation.rows {
            row.outputs = row.inputs.clone();
        }
    }
    catalog.insert_type(identity.clone()).unwrap();
    let mut wrong = request;
    if let BehaviorBinding::RepeatedSettling { behavior_type, .. } = &mut wrong.search.target {
        *behavior_type = identity.id;
    }
    let rejected = enumerate_torch_supports(
        &catalog,
        &wrong,
        BlueprintReductionBudget {
            max_layouts: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(rejected.candidates.len(), 1);
    assert_ne!(rejected.candidates[0].status, CheckStatus::Passed);
}

#[test]
fn passing_body_cannot_hide_a_broken_retained_equipment_interface() {
    let (mut catalog, mut request) = fixture();
    let base = catalog.assembly(&request.base_state).unwrap().clone();
    let mut equipment = catalog.revision(&id("original.v1")).unwrap().clone();
    equipment.id = id("broken-equipment.v1");
    equipment.behavior_bindings.clear();
    equipment
        .blocks
        .retain(|b| b.block.kind == BlockKind::Lever);
    equipment.ports = vec![port("out", Pos::new(-1, 0, 0), PortDirection::Output)];
    catalog.insert_revision(equipment).unwrap();
    request.scope = BlueprintReductionScope::Component {
        body_positions: base
            .assembly
            .blocks
            .iter()
            .filter(|b| b.block.kind != BlockKind::Lever)
            .map(|b| b.position)
            .collect(),
        environment_instances: vec![include("broken", "broken-equipment.v1")],
    };
    let result = reduce_blueprint_blocks(&catalog, &request, Default::default()).unwrap();
    assert!(
        result
            .baseline
            .behavior
            .iter()
            .any(|c| c.status == CheckStatus::Passed)
    );
    assert!(result.best.is_none());
    assert_eq!(result.stats.layouts_examined, 0);
}

fn external_route_fixture(shared_middle: bool) -> (BlueprintCatalog, BlueprintReductionRequest) {
    let (mut catalog, mut request) = fixture();
    let mut base = catalog.assembly(&request.base_state).unwrap().clone();
    let [left, middle, right] = [Pos::new(3, 0, 3), Pos::new(4, 0, 3), Pos::new(5, 0, 3)];
    let mut world = base.assembly.inspect(&catalog).unwrap().proposed_world();
    for p in [left, middle, right] {
        world.set(p.offset(0, -1, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, p);
    }
    dustroute_translate::wire::update_wire_shapes(&mut world);
    let source = catalog.revision(&id("original.v1")).unwrap().clone();
    for (name, p, direction) in [
        ("external-source.v1", left, PortDirection::Output),
        ("external-sink.v1", right, PortDirection::Input),
    ] {
        let mut definition = source.clone();
        definition.id = id(name);
        definition.behavior_bindings.clear();
        definition.blocks = vec![PositionedBlock {
            position: p,
            block: world.get(p).unwrap().clone(),
        }];
        definition.ports = vec![port("terminal", p, direction)];
        catalog.insert_revision(definition).unwrap();
    }
    let external = vec![
        include("external_source", "external-source.v1"),
        include("external_sink", "external-sink.v1"),
    ];
    base.parents = vec![base.id.clone()];
    base.id = state_id("environment-route.base.v1");
    base.assembly.instances.extend(external.clone());
    base.assembly.blocks = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    let edge = AssemblyConnection {
        source: BlueprintPortRef {
            instance: path("external_source"),
            port: "terminal".into(),
        },
        sink: BlueprintPortRef {
            instance: path("external_sink"),
            port: "terminal".into(),
        },
        path: vec![left, middle, right],
    };
    // Deliberately collides with the new body's output boundary name.
    base.assembly.boundaries.push(AssemblyBoundary {
        name: "out".into(),
        port: edge.source.clone(),
    });
    base.assembly.connections.push(edge);
    assert_eq!(
        review_assembly_in_context(
            &catalog,
            &base.assembly,
            Some(&request.behavior_context),
            BehaviorBudget::default()
        )
        .unwrap()
        .status(),
        CheckStatus::Passed
    );
    let mut body: Vec<_> = source
        .blocks
        .iter()
        .filter(|b| b.block.kind != BlockKind::Lever)
        .map(|b| b.position)
        .collect();
    if shared_middle {
        body.push(middle);
    }
    request.scope = BlueprintReductionScope::Component {
        body_positions: body,
        environment_instances: external,
    };
    request.base_state = base.id.clone();
    catalog.insert_assembly(base).unwrap();
    (catalog, request)
}

#[test]
fn enumeration_keeps_external_routes_and_boundaries_and_rejects_shared_wire_damage() {
    for shared_middle in [false, true] {
        let (catalog, request) = external_route_fixture(shared_middle);
        let before = catalog.to_json().unwrap();
        let original = &catalog.assembly(&request.base_state).unwrap().assembly;
        let report = enumerate_torch_supports(
            &catalog,
            &TorchSupportEnumerationRequest {
                search: request,
                support_position: Pos::default(),
            },
            BlueprintReductionBudget {
                max_layouts: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let entry = &report.candidates[0];
        let reviewed = entry.candidate.as_ref().unwrap();
        assert_eq!(
            reviewed.candidate.state.assembly.connections,
            original.connections
        );
        for boundary in &original.boundaries {
            assert!(
                reviewed
                    .candidate
                    .state
                    .assembly
                    .boundaries
                    .contains(boundary)
            );
        }
        assert_eq!(
            entry.status,
            if shared_middle {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            }
        );
        // The selected NOT still passes: failure comes from the shared route.
        assert!(
            reviewed
                .review
                .behavior
                .iter()
                .any(|c| c.status == CheckStatus::Passed)
        );
        if shared_middle {
            assert!(reviewed.review.arrangement.iter().any(|c| c.kind
                == dustroute_translate::promotion::CheckKind::Connection
                && c.status == CheckStatus::Failed));
        }
        assert_eq!(catalog.to_json().unwrap(), before);
    }
}

#[test]
fn supplied_alternatives_cannot_drop_or_rewrite_protected_environment_contracts() {
    let (catalog, mut request) = external_route_fixture(false);
    let report = enumerate_torch_supports(
        &catalog,
        &TorchSupportEnumerationRequest {
            search: request.clone(),
            support_position: Pos::default(),
        },
        BlueprintReductionBudget {
            max_layouts: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let candidate = report.candidates[0]
        .candidate
        .as_ref()
        .unwrap()
        .candidate
        .clone();
    request.candidate_revision = candidate.blueprint.id.clone();
    request.candidate_state = candidate.state.id.clone();
    for mutation in 0..3 {
        let mut altered = candidate.clone();
        match mutation {
            0 => altered.state.assembly.connections.clear(),
            1 => {
                altered.state.assembly.connections[0].path.remove(1);
            }
            _ => altered
                .state
                .assembly
                .boundaries
                .retain(|b| b.name != "out"),
        };
        request.alternatives = vec![altered];
        let error = reduce_blueprint_blocks(&catalog, &request, Default::default()).unwrap_err();
        assert!(error.contains("external routes and boundaries"), "{error}");
    }
}

#[test]
fn undeclared_external_endpoint_requires_explicit_reconnection_instead_of_being_erased() {
    let (catalog, mut request) = external_route_fixture(false);
    if let BlueprintReductionScope::Component {
        environment_instances,
        ..
    } = &mut request.scope
    {
        environment_instances.pop();
    }
    let before = catalog.to_json().unwrap();
    let error = enumerate_torch_supports(
        &catalog,
        &TorchSupportEnumerationRequest {
            search: request,
            support_position: Pos::default(),
        },
        Default::default(),
    )
    .unwrap_err();
    assert!(error.contains("explicit parent reconnection"), "{error}");
    assert_eq!(catalog.to_json().unwrap(), before);
}
