use std::collections::{BTreeMap, BTreeSet};

use dustroute_library::PortDirection;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_optimize::*;
use dustroute_translate::assembly::{AssemblyValidationError, validate_assembly_occurrences};
use dustroute_translate::blueprint::blueprint_cell_for_routing;
use dustroute_translate::compiler::baseline_blueprint_selection;
use dustroute_translate::promotion::{CheckStatus, review_assembly};
use dustroute_translate::{
    cell_library::CellLibrary, compiler::BaselineCompiler, ir::DagBuilder, ir::GateKind,
    world::Block, world::BlockKind, world::Pos, world::Region,
};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn instance(value: &str) -> InstanceId {
    InstanceId::new(value).unwrap()
}

fn inverter() -> dustroute_translate::ir::LogicDag {
    let mut builder = DagBuilder::new();
    let input = builder.input("in");
    let output = builder.gate(GateKind::Not, &[input], None);
    builder.finish([("out".into(), output)]).unwrap()
}

fn nested_library(broken: bool) -> (CellLibrary, BlueprintRevisionId) {
    let mut catalog = builtin_blueprints().clone();
    let classification = ClassificationRevisionId::new("local.interpretation.v1").unwrap();
    catalog
        .insert_classification(ClassificationRevision {
            id: classification.clone(),
            name: "Local interpretation without logical claims".into(),
            logical_claim: None,
        })
        .unwrap();
    let mut child = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    child.id = id("local.child.v1");
    child
        .ports
        .iter_mut()
        .find(|port| port.direction == PortDirection::Input)
        .unwrap()
        .required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(child.clone()).unwrap();
    let mut shared = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    shared.id = id("local.shared.v1");
    if broken {
        let torch = child
            .blocks
            .iter()
            .find(|record| record.block.kind == BlockKind::RedstoneTorch)
            .unwrap()
            .position;
        shared.blocks = vec![PositionedBlock {
            position: torch,
            block: Block::new(BlockKind::Air),
        }];
        shared.ports.clear();
        shared.classifications.clear();
    }
    catalog.insert_revision(shared.clone()).unwrap();
    let mut parent = child.clone();
    parent.id = id("local.parent.v1");
    parent.classifications = vec![classification.clone()];
    parent.blocks.clear();
    let world = catalog.expand(&child.id).unwrap().proposed_world();
    let (min, max) = world.bounds().unwrap();
    parent.initial_layout = Some(BlueprintLayout {
        blocks: child.blocks.clone(),
        known_regions: vec![Region::new(min, max)],
    });
    parent.inclusions = [(&child.id, "b"), (&shared.id, "c")]
        .into_iter()
        .map(|(revision, name)| BlueprintInclusion {
            instance: instance(name),
            revision: revision.clone(),
            origin: Pos::default(),
            rotation: Default::default(),
        })
        .collect();
    // Expose the child's interface without changing the child's immutable type.
    parent.port_bindings = parent
        .ports
        .iter()
        .map(|port| BlueprintPortBinding {
            name: port.name.clone(),
            port: BlueprintPortRef {
                instance: vec![instance("b")],
                port: port.name.clone(),
            },
        })
        .collect();
    for port in &mut parent.ports {
        port.required_source_types.clear();
    }
    catalog.insert_revision(parent.clone()).unwrap();
    let catalog = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    (
        CellLibrary::from_blueprints(
            &catalog,
            &BTreeMap::from([(GateKind::Not, classification)]),
            64,
        )
        .unwrap(),
        parent.id,
    )
}

fn boundaries() -> BTreeMap<GateKind, BlueprintRevisionId> {
    baseline_blueprint_selection()
        .into_iter()
        .filter(|(kind, _)| matches!(kind, GateKind::Input | GateKind::Output))
        .collect()
}

#[test]
fn catalog_discovery_compiles_nested_shared_candidates_without_builtin_registration() {
    let (library, parent) = nested_library(false);
    assert!(library.unavailable().is_empty());
    assert_eq!(library.candidates_for(GateKind::Not).len(), 1);
    let catalog = library.catalog().unwrap();
    let saved = catalog.to_json().unwrap();
    let compiled = BaselineCompiler::new(Default::default())
        .compile_with_library(&inverter(), &library, &boundaries())
        .unwrap();
    assert!(
        compiled
            .blueprint_revisions
            .values()
            .any(|revision| *revision == parent)
    );
    let assembly = compiled.assembly.as_ref().unwrap();
    let report = review_assembly(catalog, assembly).unwrap();
    assert_eq!(report.status(), CheckStatus::Passed);
    assert!(
        report
            .occurrences
            .keys()
            .any(|path| path.last() == Some(&instance("c")))
    );
    // Library labels did not supply behavior: verify the physical circuit separately.
    for input in [false, true] {
        let mut world = compiled.world.clone().into_world();
        if input {
            world.set(
                compiled.input_positions["in"].offset(-1, 0, 0),
                Block::new(BlockKind::RedstoneBlock),
            );
        }
        dustroute_translate::wire::update_wire_shapes(&mut world);
        let settled = dustroute_translate::sim::RedstoneTickSimulator::new(world)
            .unwrap()
            .settle_ticks(64)
            .unwrap();
        assert_eq!(
            settled.strength(compiled.output_positions["out"]) > 0,
            !input
        );
    }
    let realized = realize_staged_optimization_with_library(
        &compiled.physical,
        &compiled.world,
        &compiled.routing,
        &OptimizationPlan { phases: vec![] },
        Default::default(),
        &library,
    )
    .unwrap();
    let captured = realized.capture_assembly_in_catalog(catalog).unwrap();
    assert_eq!(
        review_assembly(catalog, &captured).unwrap().status(),
        CheckStatus::Passed
    );
    assert_eq!(catalog.to_json().unwrap(), saved);
}

#[test]
fn parent_behavior_pass_does_not_admit_a_broken_shared_child() {
    let (library, parent) = nested_library(true);
    // The old Boolean candidate filter would accept this geometry.
    assert_eq!(
        library
            .choose(GateKind::Not)
            .unwrap()
            .source_revision
            .as_ref(),
        Some(&parent)
    );
    let result = BaselineCompiler::new(Default::default()).compile_with_library(
        &inverter(),
        &library,
        &boundaries(),
    );
    let Err(dustroute_translate::compiler::CompileError::Review(report)) = result else {
        panic!("broken child must stop compilation: {result:?}")
    };
    assert!(
        report
            .occurrences
            .iter()
            .any(|(path, review)| path.last() == Some(&instance("c"))
                && review.status() == CheckStatus::Failed)
    );
    assert!(
        report
            .occurrences
            .values()
            .any(|review| review.revision == parent && review.status() == CheckStatus::Passed)
    );

    // The optimizer must reject the same candidate even though it is smaller.
    let mut selection = baseline_blueprint_selection();
    selection.insert(GateKind::Not, id(NOT_SIDE_REVISION));
    let catalog = library.catalog().unwrap();
    let original = BaselineCompiler::new(Default::default())
        .compile_with_blueprints(&inverter(), catalog, &selection)
        .unwrap();
    let plan = OptimizationPlan {
        phases: vec![OptimizationPhase::GlobalCompact {
            max_steps: 1,
            move_step: 0,
            weights: Default::default(),
        }],
    };
    let result = realize_staged_optimization_with_library(
        &original.physical,
        &original.world,
        &original.routing,
        &plan,
        Default::default(),
        &library,
    )
    .unwrap();
    assert!(
        result.optimization.circuit.cells.values().all(|node| node
            .placed
            .cell
            .source_revision
            .as_ref()
            != Some(&parent))
    );
    assert!(
        original
            .blueprint_revisions
            .values()
            .any(|revision| *revision == id(NOT_SIDE_REVISION))
    );
}

#[test]
fn typed_macro_requires_explicit_upstream_context_and_keeps_shared_occurrences() {
    let (library, parent) = nested_library(false);
    let catalog = library.catalog().unwrap();
    let compiled = BaselineCompiler::new(Default::default())
        .compile_with_library(&inverter(), &library, &boundaries())
        .unwrap();
    let (cell_id, node) = compiled
        .physical
        .cells
        .iter()
        .find(|(_, node)| node.logical_kind == GateKind::Not)
        .unwrap();
    let mut boundary = extract_cell_boundary(&node.placed.cell);
    for port in &mut boundary {
        port.position = match port.direction {
            MacroBoundaryDirection::Input => node.placed.input_port(&port.name).unwrap().pos,
            MacroBoundaryDirection::Output => node.placed.output_port(&port.name).unwrap().pos,
        };
    }
    let plan = plan_blueprint_replacement(
        catalog,
        &parent,
        &["a".into()],
        &["out".into()],
        &boundary,
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(plan.placed.origin, node.placed.origin);
    let replaceable = node.placed.blocks().map(|(position, _)| position).collect();
    assert!(matches!(
        materialize_macro_replacement(&plan, &compiled.world, &replaceable, 14),
        Err(MacroRealizationError::Blueprint(_))
    ));
    let assembly = compiled.assembly.as_ref().unwrap();
    let before = assembly.clone();
    let saved = catalog.to_json().unwrap();
    let occurrence = vec![instance(&format!("cell-{}", cell_id.0))];
    let realized = materialize_macro_replacement_in_assembly(
        &plan,
        catalog,
        assembly,
        &occurrence,
        &replaceable,
        14,
    )
    .unwrap();
    assert_eq!(realized.assembly.instances, assembly.instances);
    assert_eq!(
        review_assembly(catalog, &realized.assembly)
            .unwrap()
            .status(),
        CheckStatus::Passed
    );
    assert!(!plan.automatic_apply_allowed);
    assert_eq!(*assembly, before);
    assert_eq!(catalog.to_json().unwrap(), saved);

    let mut unknown = realized.assembly.clone();
    unknown.known_regions.clear();
    // An explicit air claim outside the known context remains undetermined.
    let mut changed_catalog = catalog.clone();
    let mut clearance = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    clearance.id = id("unknown.clearance.v1");
    clearance.blocks = vec![PositionedBlock {
        position: Pos::new(999, 0, 0),
        block: Block::new(BlockKind::Air),
    }];
    clearance.ports.clear();
    changed_catalog.insert_revision(clearance.clone()).unwrap();
    unknown.instances.push(BlueprintInclusion {
        instance: instance("unknown"),
        revision: clearance.id,
        origin: Pos::default(),
        rotation: Default::default(),
    });
    let report = review_assembly(&changed_catalog, &unknown).unwrap();
    assert_eq!(report.status(), CheckStatus::Undetermined);
    assert_eq!(
        report.occurrences[&vec![instance("unknown")]].status(),
        CheckStatus::Undetermined
    );
    // The stronger initial gate now also sees missing clearance for explicit
    // wire rises, so it stops before the later occurrence-only review gate.
    assert!(matches!(
        validate_assembly_occurrences(&changed_catalog, &unknown),
        Err(AssemblyValidationError::World(error)) if error.issues.iter().any(|issue|
            matches!(issue, dustroute_translate::world::WorldValidationIssue::UnknownWireConnection { .. }))
    ));
}

#[test]
fn multiple_inputs_and_outputs_need_no_gate_kind_or_logical_claim() {
    let mut catalog = builtin_blueprints().clone();
    let leaf = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    let mut pair = leaf.clone();
    pair.id = id("local.pair.v1");
    pair.classifications.clear();
    pair.blocks.clear();
    pair.ports.clear();
    for (name, x) in [("left", 0), ("right", 10)] {
        pair.inclusions.push(BlueprintInclusion {
            instance: instance(name),
            revision: leaf.id.clone(),
            origin: Pos::new(x, 0, 0),
            rotation: Default::default(),
        });
        for child_port in &leaf.ports {
            let mut port = child_port.clone();
            port.name = format!("{name}_{}", port.name);
            port.position.x += x;
            pair.port_bindings.push(BlueprintPortBinding {
                name: port.name.clone(),
                port: BlueprintPortRef {
                    instance: vec![instance(name)],
                    port: child_port.name.clone(),
                },
            });
            pair.ports.push(port);
        }
    }
    catalog.insert_revision(pair.clone()).unwrap();
    let cell = blueprint_cell_for_routing(&catalog, &pair.id).unwrap();
    let plan = plan_blueprint_replacement(
        &catalog,
        &pair.id,
        &["left_a".into(), "right_a".into()],
        &["left_out".into(), "right_out".into()],
        &extract_cell_boundary(&cell),
        &BTreeSet::new(),
    )
    .unwrap();
    let result =
        materialize_macro_replacement(&plan, &cell.world, &cell.world.positions().collect(), 14)
            .unwrap();
    assert_eq!(
        result.assembly.inspect(&catalog).unwrap().occurrences.len(),
        3
    );
    assert_eq!(result.world, cell.world);
    assert_eq!(
        plan.verification.steady_state,
        ContextualVerificationState::Pending
    );
}

#[test]
fn macro_materialization_rejects_a_shared_child_without_repairing_or_dropping_it() {
    let (library, parent) = nested_library(true);
    let catalog = library.catalog().unwrap();
    let compiled = BaselineCompiler::new(Default::default())
        .compile_with_blueprints(&inverter(), catalog, &baseline_blueprint_selection())
        .unwrap();
    let (cell_id, node) = compiled
        .physical
        .cells
        .iter()
        .find(|(_, node)| node.logical_kind == GateKind::Not)
        .unwrap();
    let mut boundary = extract_cell_boundary(&node.placed.cell);
    for port in &mut boundary {
        port.position = match port.direction {
            MacroBoundaryDirection::Input => node.placed.input_port(&port.name).unwrap().pos,
            MacroBoundaryDirection::Output => node.placed.output_port(&port.name).unwrap().pos,
        };
    }
    let plan = plan_blueprint_replacement(
        catalog,
        &parent,
        &["a".into()],
        &["out".into()],
        &boundary,
        &BTreeSet::new(),
    )
    .unwrap();
    let mut context = compiled.assembly.clone().unwrap();
    let selected = instance(&format!("cell-{}", cell_id.0));
    context
        .instances
        .iter_mut()
        .find(|occurrence| occurrence.instance == selected)
        .unwrap()
        .revision = parent;
    let snapshot = context.clone();
    let replaceable = node.placed.blocks().map(|(position, _)| position).collect();
    let error = materialize_macro_replacement_in_assembly(
        &plan,
        catalog,
        &context,
        &vec![selected],
        &replaceable,
        14,
    )
    .unwrap_err();
    assert!(
        matches!(error, MacroRealizationError::Blueprint(ref details) if details.contains("local.shared.v1") && details.contains("Failed")),
        "{error:?}"
    );
    assert_eq!(context, snapshot);
    assert!(catalog.revision(&id("local.shared.v1")).is_some());
}

#[test]
fn unsupported_interfaces_are_reported_and_new_revisions_do_not_rebind_existing_libraries() {
    let (library, parent) = nested_library(false);
    let mut newer = library.catalog().unwrap().clone();
    let mut mechanical = newer.revision(&parent).unwrap().clone();
    mechanical.id = id("local.mechanical.v1");
    mechanical.port_bindings.clear();
    mechanical.ports[1].kind = BlueprintPortKind::BlockState;
    newer.insert_revision(mechanical.clone()).unwrap();
    let classification = ClassificationRevisionId::new("local.interpretation.v1").unwrap();
    let current = CellLibrary::from_blueprints(
        &newer,
        &BTreeMap::from([(GateKind::Not, classification)]),
        64,
    )
    .unwrap();
    assert_eq!(current.unavailable().len(), 1);
    assert_eq!(current.unavailable()[0].0, mechanical.id);
    assert!(
        library
            .catalog()
            .unwrap()
            .revision(&mechanical.id)
            .is_none()
    );
    assert_eq!(
        library.candidates_for(GateKind::Not),
        current.candidates_for(GateKind::Not)
    );
    assert_eq!(
        library
            .choose(GateKind::Not)
            .unwrap()
            .source_revision
            .as_ref(),
        Some(&parent)
    );
}
