#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::assembly::*;
use dustroute_library::behavior_type::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::{
    NOT_SIDE_REVISION, NOT_TOP_REVISION, builtin_blueprints,
};
use dustroute_library::builtin_laws::{
    DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws, torch_law_revision,
};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, RotationY, World};
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::promotion::*;

fn id(name: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(name).unwrap()
}
fn type_id(name: &str) -> TypeRevisionId {
    TypeRevisionId::new(name).unwrap()
}
fn path(name: &str) -> InstancePath {
    vec![InstanceId::new(name).unwrap()]
}
fn include(name: &str, source: &str) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision: id(source),
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}
fn port(
    name: &str,
    position: Pos,
    kind: BlueprintPortKind,
    direction: PortDirection,
) -> BlueprintPort {
    BlueprintPort {
        name: name.into(),
        position,
        kind,
        direction,
        facing: None,
        required_source_types: vec![],
    }
}
fn context(position: Pos, kind: BlueprintPortKind, lever: Pos) -> PhysicalBehaviorContext {
    PhysicalBehaviorContext {
        profile: PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        dust_law: id(DUST_LAW_REVISION),
        torch_law: id(TORCH_LAW_REVISION),
        max_electrical_iterations: 128,
        input_drivers: vec![PhysicalInputDriver {
            port_position: position,
            port_kind: kind,
            lever_position: lever,
        }],
    }
}
fn definition(invert: bool) -> TypeRevision {
    TypeRevision {
        id: type_id(if invert {
            "relation.not.v1"
        } else {
            "relation.identity.v1"
        }),
        name: "Behavior, independent of label".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["a".into()],
                outputs: vec!["out".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|b| BooleanRow {
                        inputs: vec![b],
                        outputs: vec![b ^ invert],
                    })
                    .collect(),
            },
        },
    }
}
fn binding(invert: bool) -> BehaviorBinding {
    BehaviorBinding::RepeatedSettling {
        behavior_type: definition(invert).id,
        inputs: BTreeMap::from([("a".into(), "input".into())]),
        outputs: BTreeMap::from([("out".into(), "output".into())]),
    }
}

#[test]
fn explicit_location_bindings_cannot_inherit_a_legacy_fixed_geometry_pass() {
    use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
    let (mut catalog, mut assembly, context) = fixture();
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = id("location.interpretation.v1");
    source
        .ports
        .iter_mut()
        .find(|p| p.name == "output")
        .unwrap()
        .kind = BlueprintPortKind::BlockState;
    source.behavior_bindings = vec![BehaviorBinding::Observed {
        behavior_type: definition(true).id,
        observed_inputs: BTreeMap::from([(
            "a".into(),
            ObservedPort::Signal {
                port: "input".into(),
            },
        )]),
        observed_outputs: BTreeMap::from([(
            "out".into(),
            ObservedPort::Location {
                port: "output".into(),
                predicate: LocationPredicate::Present,
            },
        )]),
    }];
    assembly.instances[0].revision = source.id.clone();
    catalog.insert_revision(source).unwrap();
    let before = catalog.fixture_json().unwrap();
    let report = review_assembly_in_context(
        &catalog,
        &assembly,
        Some(&context),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.behavior_status(), Some(CheckStatus::Undetermined));
    assert_ne!(report.status(), CheckStatus::Passed);
    assert!(
        report
            .behavior
            .iter()
            .any(|check| check.detail.contains("runtime behavior context"))
    );
    assert_eq!(catalog.fixture_json().unwrap(), before);
}
fn fixture() -> (BlueprintCatalog, Assembly, PhysicalBehaviorContext) {
    let mut catalog = builtin_laws().clone();
    catalog.insert_type(definition(true)).unwrap();
    catalog.insert_type(definition(false)).unwrap();
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    let lever = Pos::new(-1, 0, 0);
    let b = world.place(BlockKind::Lever, lever);
    b.support_offset = Some(Pos::new(1, 0, 0));
    b.powered = Some(false);
    let torch = Pos::new(1, 0, 0);
    let b = world.place(BlockKind::RedstoneTorch, torch);
    b.support_offset = Some(Pos::new(-1, 0, 0));
    b.facing = Some(Facing::East);
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    let blocks: Vec<_> = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    let blueprint = BlueprintRevision {
        id: id("source.v1"),
        parents: vec![],
        name: "Declared NOT".into(),
        classifications: vec![],
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![binding(true)],
        blocks: blocks
            .iter()
            .filter(|b| b.position != lever)
            .cloned()
            .collect(),
        initial_layout: None,
        law: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        ports: vec![
            port(
                "input",
                Pos::default(),
                BlueprintPortKind::BlockPower,
                PortDirection::Input,
            ),
            port(
                "output",
                Pos::new(1, 1, 0),
                BlueprintPortKind::BlockPower,
                PortDirection::Output,
            ),
        ],
        provenance: torch_law_revision().provenance.clone(),
    };
    catalog.insert_revision(blueprint).unwrap();
    (
        catalog,
        Assembly {
            name: "Actual NOT and driver".into(),
            instances: vec![include("gate", "source.v1")],
            blocks,
            known_regions: vec![Region::new(Pos::new(-5, -5, -5), Pos::new(5, 5, 5))],
            connections: vec![],
            boundaries: vec![],
        },
        context(Pos::default(), BlueprintPortKind::BlockPower, lever),
    )
}
fn review(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: &PhysicalBehaviorContext,
) -> PromotionReport {
    review_assembly_in_context(catalog, assembly, Some(context), BehaviorBudget::default()).unwrap()
}
fn grouping() -> BlueprintGrouping {
    BlueprintGrouping {
        id: id("group.v1"),
        name: "Preserved declarations".into(),
        classifications: vec![],
        provenance: torch_law_revision().provenance.clone(),
    }
}

fn declared_law_fixture() -> (BlueprintCatalog, Assembly, PhysicalBehaviorContext) {
    let (mut catalog, mut assembly, context) = fixture();
    let mut source = catalog.revision(&id("source.v1")).unwrap().clone();
    source.id = id("declared.v1");
    source.behavior_bindings.clear();
    source.required_laws = context.law_revisions().into_iter().cloned().collect();
    catalog.insert_revision(source).unwrap();
    assembly.instances[0].revision = id("declared.v1");
    (catalog, assembly, context)
}

#[test]
fn law_requirements_need_a_world_and_parent_success_cannot_hide_child_conflicts() {
    use dustroute_translate::assembly::validate_assembly;
    let (mut catalog, mut assembly, context) = declared_law_fixture();
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Undetermined
    );
    assert!(validate_assembly(&catalog, &assembly).is_err());
    assert_eq!(
        review(&catalog, &assembly, &context).status(),
        CheckStatus::Passed
    );

    let mut alternative = torch_law_revision().clone();
    alternative.id = id("unselected-torch.v1");
    catalog.insert_revision(alternative.clone()).unwrap();
    let mut child = catalog.revision(&id("declared.v1")).unwrap().clone();
    child.id = id("conflicting-child.v1");
    child.required_laws = vec![alternative.id];
    catalog.insert_revision(child).unwrap();
    let mut parent = catalog.revision(&id("declared.v1")).unwrap().clone();
    parent.id = id("parent-with-conflict.v1");
    parent.inclusions = vec![include("child", "conflicting-child.v1")];
    catalog.insert_revision(parent).unwrap();
    assembly.instances[0].revision = id("parent-with-conflict.v1");
    let report = review(&catalog, &assembly, &context);
    assert_eq!(
        report.occurrences[&path("gate")].status(),
        CheckStatus::Passed
    );
    let mut child_path = path("gate");
    child_path.extend(path("child"));
    assert_eq!(
        report.occurrences[&child_path].status(),
        CheckStatus::Failed
    );
    assert_eq!(report.status(), CheckStatus::Failed);
    let before = catalog.fixture_json().unwrap();
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping()).unwrap();
    let checked = candidate
        .validate_in_context(&catalog, context, BehaviorBudget::default())
        .unwrap();
    assert!(checked.adopt(&mut catalog).is_err());
    assert_eq!(catalog.fixture_json().unwrap(), before);
}

#[test]
fn publishing_device_laws_does_not_extend_dust_torch_proof_contexts() {
    for law in dustroute_minecraft::repeater_law::REPEATER_LAW_IDS
        .into_iter()
        .chain(std::iter::once(
            dustroute_minecraft::comparator_law::COMPARATOR_LAW_REVISION,
        ))
        .chain(std::iter::once(
            dustroute_minecraft::observer_law::OBSERVER_LAW_REVISION,
        ))
        .chain(dustroute_minecraft::lamp_law::LAMP_LAW_IDS)
        .chain(dustroute_minecraft::piston_law::PISTON_LAW_IDS)
    {
        let (mut catalog, mut assembly, context) = fixture();
        assert!(catalog.revision(&id(law)).is_some());
        assert!(!context.law_revisions().contains(&&id(law)));
        let mut source = catalog.revision(&id("source.v1")).unwrap().clone();
        source.id = id("requires-device.v1");
        source.required_laws = vec![id(law)];
        catalog.insert_revision(source).unwrap();
        assembly.instances[0].revision = id("requires-device.v1");
        let before = catalog.fixture_json().unwrap();
        let report = review(&catalog, &assembly, &context);
        assert_eq!(report.status(), CheckStatus::Failed);
        assert!(
            report.occurrences[&path("gate")]
                .checks
                .iter()
                .any(|check| check.kind == CheckKind::PhysicalLaw
                    && check.detail.contains("not selected"))
        );
        let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping()).unwrap();
        let reviewed = candidate
            .validate_in_context(&catalog, context, BehaviorBudget::default())
            .unwrap();
        assert!(reviewed.adopt(&mut catalog).is_err());
        assert_eq!(catalog.fixture_json().unwrap(), before);
    }
}

#[test]
fn matching_revision_ids_do_not_certify_an_unsupported_world_adapter() {
    use dustroute_minecraft::law::{Expr, Instruction};
    let (mut catalog, mut assembly, mut context) = declared_law_fixture();
    let mut unsupported = torch_law_revision().clone();
    unsupported.id = id("unsupported-torch.v1");
    unsupported
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("neighbor_update")
        .unwrap()
        .push(Instruction::Set {
            register: "lit".into(),
            value: Expr::Constant { value: 0 },
        });
    catalog.insert_revision(unsupported.clone()).unwrap();
    context.torch_law = unsupported.id.clone();
    let mut source = catalog.revision(&id("declared.v1")).unwrap().clone();
    source.id = id("unsupported-source.v1");
    source.required_laws = vec![unsupported.id];
    catalog.insert_revision(source).unwrap();
    assembly.instances[0].revision = id("unsupported-source.v1");
    let report = review(&catalog, &assembly, &context);
    assert_eq!(report.status(), CheckStatus::Undetermined);
    assert!(
        report.occurrences[&path("gate")]
            .checks
            .iter()
            .any(|check| check.kind == CheckKind::PhysicalLaw
                && check.detail.contains("block-effects profile"))
    );
}

fn runtime(
    catalog: &mut BlueprintCatalog,
    assembly: Assembly,
    context: &PhysicalBehaviorContext,
    state_id: &str,
) -> Result<dustroute_translate::physical_behavior::PhysicalBehaviorModel, String> {
    use dustroute_translate::physical_behavior::*;
    let assembly_id = AssemblyRevisionId::new(state_id).unwrap();
    catalog
        .insert_assembly(AssemblyRevision {
            id: assembly_id.clone(),
            parents: vec![],
            assembly,
        })
        .unwrap();
    PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        catalog,
        PhysicalBehaviorSelection {
            assembly: assembly_id,
            behavior_type: definition(true).id,
            dust_law: context.dust_law.clone(),
            torch_law: context.torch_law.clone(),
            inputs: BTreeMap::from([("a".into(), Pos::new(-1, 0, 0))]),
            outputs: BTreeMap::from([(
                "out".into(),
                PhysicalOutput::BlockPower {
                    position: Pos::new(1, 1, 0),
                },
            )]),
            max_electrical_iterations: context.max_electrical_iterations,
        },
        context.profile,
    )
}

#[test]
fn direct_execution_checks_transitive_law_requirements() {
    let (mut catalog, mut assembly, mut context) = declared_law_fixture();
    let mut additional = torch_law_revision().clone();
    additional.id = id("unselected-dependency.v1");
    catalog.insert_revision(additional.clone()).unwrap();
    let mut selected = torch_law_revision().clone();
    selected.id = id("dependent-torch.v1");
    selected.required_laws = vec![additional.id];
    catalog.insert_revision(selected.clone()).unwrap();
    context.torch_law = selected.id.clone();
    let mut source = catalog.revision(&id("declared.v1")).unwrap().clone();
    source.id = id("dependent-source.v1");
    source.required_laws = vec![selected.id];
    catalog.insert_revision(source).unwrap();
    assembly.instances[0].revision = id("dependent-source.v1");
    assert_eq!(
        review(&catalog, &assembly, &context).status(),
        CheckStatus::Failed
    );
    assert!(
        runtime(&mut catalog, assembly, &context, "dependent-state.v1")
            .unwrap_err()
            .contains("unselected-dependency.v1")
    );
}

#[test]
fn implicit_spatial_pins_preserve_old_archives_and_reject_conflicting_definitions() {
    use dustroute_library::builtin_laws::spatial_law_revisions;
    use dustroute_minecraft::law::{Expr, Instruction};
    let (catalog, assembly, context) = fixture();
    let old_context = serde_json::to_string(&context).unwrap();
    assert!(!old_context.contains("spatial"));
    assert_eq!(context.law_revisions().len(), 6);
    let mut archive: serde_json::Value =
        serde_json::from_str(&catalog.fixture_json().unwrap()).unwrap();
    archive["revisions"]
        .as_array_mut()
        .unwrap()
        .retain(|record| {
            !spatial_law_revisions()
                .iter()
                .any(|id| record["id"] == id.as_str())
        });
    let old_catalog = archive_fixture::catalog(&archive.to_string()).unwrap();
    let old_saved = old_catalog.fixture_json().unwrap();
    assert_eq!(
        review(&old_catalog, &assembly, &context).status(),
        CheckStatus::Passed
    );
    assert_eq!(old_catalog.fixture_json().unwrap(), old_saved);
    assert_eq!(serde_json::to_string(&context).unwrap(), old_context);

    let mut conflict = builtin_laws()
        .revision(&spatial_law_revisions()[0])
        .unwrap()
        .clone();
    conflict
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "dust_support".into(),
            value: Expr::Constant { value: 0 },
        });
    let mut incompatible = old_catalog.clone();
    incompatible.insert_revision(conflict).unwrap();
    assert_eq!(
        review(&incompatible, &assembly, &context).status(),
        CheckStatus::Undetermined
    );
    assert!(
        runtime(&mut incompatible, assembly.clone(), &context, "conflict.v1")
            .unwrap_err()
            .contains("immutable spatial law")
    );

    // A genuinely new law can be stored, but current fixed profiles cannot
    // silently substitute it or pass a source that requires that other model.
    let mut changed = builtin_laws()
        .revision(&spatial_law_revisions()[0])
        .unwrap()
        .clone();
    changed.id = id("custom-spatial.v1");
    changed.parents = vec![spatial_law_revisions()[0].clone()];
    let mut candidate = catalog.revision(&id("source.v1")).unwrap().clone();
    candidate.id = id("new-spatial-source.v1");
    candidate.required_laws = vec![changed.id.clone()];
    let mut catalog = catalog;
    catalog
        .insert_revisions(vec![changed, candidate.clone()])
        .unwrap();
    let mut proposed = assembly;
    proposed.instances[0].revision = candidate.id;
    assert_eq!(
        review(&catalog, &proposed, &context).status(),
        CheckStatus::Failed
    );
}

#[test]
fn shared_nested_declarations_do_not_duplicate_world_history_or_scheduled_events() {
    use dustroute_translate::behavior_type::BehaviorModel;
    let (mut catalog, mut assembly, mut context) = declared_law_fixture();
    let mut dust = catalog.revision(&context.dust_law).unwrap().clone();
    let mut torch = catalog.revision(&context.torch_law).unwrap().clone();
    dust.id = id("mutual-dust.v1");
    torch.id = id("mutual-torch.v1");
    dust.required_laws = vec![torch.id.clone()];
    torch.required_laws = vec![dust.id.clone()];
    context.dust_law = dust.id.clone();
    context.torch_law = torch.id.clone();
    catalog.insert_revisions(vec![dust, torch]).unwrap();
    let mut source = catalog.revision(&id("declared.v1")).unwrap().clone();
    source.id = id("mutual-source.v1");
    source.required_laws = context.law_revisions().into_iter().cloned().collect();
    catalog.insert_revision(source.clone()).unwrap();
    assembly.instances[0].revision = source.id.clone();
    let single = runtime(&mut catalog, assembly.clone(), &context, "single.v1").unwrap();

    let mut parent = source.clone();
    parent.id = id("nested-source.v1");
    parent.inclusions = vec![
        include("first", source.id.as_str()),
        include("second", source.id.as_str()),
    ];
    catalog.insert_revision(parent.clone()).unwrap();
    assembly
        .instances
        .push(include("overlapping", parent.id.as_str()));
    assert_eq!(
        review(&catalog, &assembly, &context).status(),
        CheckStatus::Passed
    );
    let shared = runtime(&mut catalog, assembly, &context, "shared.v1").unwrap();
    let (mut left, mut right) = (
        single.initial_state().unwrap(),
        shared.initial_state().unwrap(),
    );
    let torch = Pos::new(1, 0, 0);
    // Eight rapid toggles exercise burnout history and its queued recovery;
    // compare all law registers, histories and events on every game tick.
    for (powered, ticks) in (0..16).map(|i| (i % 2 == 0, 2)).chain([(false, 180)]) {
        left = single.with_inputs(&left, &[powered]).unwrap();
        right = shared.with_inputs(&right, &[powered]).unwrap();
        for _ in 0..ticks {
            assert_eq!(
                single.torch_state(&left, torch).unwrap(),
                shared.torch_state(&right, torch).unwrap()
            );
            assert_eq!(
                single.outputs(&left).unwrap(),
                shared.outputs(&right).unwrap()
            );
            left = single.step(&left).unwrap();
            right = shared.step(&right).unwrap();
        }
    }
}

#[test]
fn arbitrary_input_not_promotes_with_fresh_proof_and_keeps_immutable_sources() {
    let (mut catalog, assembly, context) = fixture();
    let original = catalog.revision(&id("source.v1")).unwrap().clone();
    let unknown = review_assembly(&catalog, &assembly).unwrap();
    assert_eq!(unknown.behavior_status(), Some(CheckStatus::Undetermined));
    let report = review(&catalog, &assembly, &context);
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    assert!(
        report.behavior[0]
            .detail
            .contains("universal-overapproximate-repeated-settling.v1")
    );
    assert!(
        report.behavior[0]
            .detail
            .contains("\"abstract_states\":9520")
    );
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping()).unwrap();
    let checked = candidate
        .validate_in_context(&catalog, context.clone(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(checked.report().status(), CheckStatus::Passed);
    let adopted = checked.adopt(&mut catalog).unwrap();
    assert_eq!(adopted.blocks, assembly.blocks);
    assert_eq!(catalog.revision(&original.id), Some(&original));
    let saved = catalog.fixture_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    let loaded = archive_fixture::catalog(&saved).unwrap();
    assert_eq!(loaded.fixture_json().unwrap(), saved);
    assert_eq!(
        review(&loaded, &adopted, &context).status(),
        CheckStatus::Passed
    );
    for version in 1..6 {
        assert!(
            archive_fixture::catalog(&saved.replace("catalog.v13", &format!("catalog.v{version}")))
                .is_err()
        );
    }
}

#[test]
fn missing_ambiguous_unrelated_drivers_wrong_ports_and_budgets_cannot_pass() {
    let (catalog, assembly, ctx) = fixture();
    for mode in 0..5 {
        let mut context = ctx.clone();
        let mut assembly = assembly.clone();
        match mode {
            0 => context.input_drivers.clear(),
            1 => context.input_drivers.push(context.input_drivers[0].clone()),
            2 => context.input_drivers[0].port_kind = BlueprintPortKind::Wire,
            3 => context.input_drivers[0].lever_position = Pos::default(),
            _ => {
                // A valid remote lever exists, but does not drive the named input.
                let support = Pos::new(3, 0, 0);
                let lever = Pos::new(3, 1, 0);
                assembly.blocks.push(PositionedBlock {
                    position: support,
                    block: Block::new(BlockKind::Solid),
                });
                let mut world = World::new();
                world.set(support, Block::new(BlockKind::Solid));
                let block = world.place(BlockKind::Lever, lever).clone();
                assembly.blocks.push(PositionedBlock {
                    position: lever,
                    block,
                });
                context.input_drivers[0].lever_position = lever;
            }
        }
        let report = review(&catalog, &assembly, &context);
        assert_eq!(
            report.behavior_status(),
            Some(CheckStatus::Undetermined),
            "mode {mode}: {report:?}"
        );
        if mode == 4 {
            assert!(report.behavior[0].detail.contains("does not establish"));
        }
    }
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping()).unwrap();
    let checked = candidate
        .validate_in_context(
            &catalog,
            ctx,
            BehaviorBudget {
                max_states: 1,
                ..BehaviorBudget::default()
            },
        )
        .unwrap();
    assert_eq!(
        checked.report().behavior_status(),
        Some(CheckStatus::Undetermined)
    );
    let mut untouched = catalog.clone();
    assert!(checked.adopt(&mut untouched).is_err());
    assert_eq!(
        catalog.fixture_json().unwrap(),
        untouched.fixture_json().unwrap()
    );
}

#[test]
fn parent_pass_never_hides_child_relation_failure_or_rebinds_its_type() {
    let (mut catalog, mut assembly, context) = fixture();
    let source = catalog.revision(&id("source.v1")).unwrap().clone();
    let mut child = source.clone();
    child.id = id("child.v1");
    child.behavior_bindings = vec![binding(false)];
    catalog.insert_revision(child.clone()).unwrap();
    let mut parent = source;
    parent.id = id("parent.v1");
    parent.blocks.clear();
    parent.inclusions = vec![include("child", "child.v1")];
    catalog.insert_revision(parent).unwrap();
    assembly.instances = vec![include("root", "parent.v1")];
    let checked = PromotionCandidate::prepare(&catalog, &assembly, grouping())
        .unwrap()
        .validate_in_context(&catalog, context, BehaviorBudget::default())
        .unwrap();
    let report = checked.report();
    assert!(
        report
            .behavior
            .iter()
            .any(|c| c.status == CheckStatus::Passed)
    );
    assert!(
        report
            .behavior
            .iter()
            .any(|c| c.status == CheckStatus::Undetermined)
    );
    // Abstract counterexamples remain undetermined until concretely replayed.
    assert_eq!(report.status(), CheckStatus::Undetermined);
    let before = catalog.fixture_json().unwrap();
    assert!(checked.adopt(&mut catalog).is_err());
    assert_eq!(catalog.fixture_json().unwrap(), before);
    assert_eq!(catalog.revision(&child.id), Some(&child));
}

#[test]
fn existing_not_layouts_support_transformed_ports_without_changing_type_coordinates() {
    for source_id in [NOT_TOP_REVISION, NOT_SIDE_REVISION] {
        let mut catalog = builtin_blueprints().clone();
        for law in builtin_laws().revisions() {
            catalog.insert_revision(law.clone()).unwrap();
        }
        catalog.insert_type(definition(true)).unwrap();
        let mut source = catalog.revision(&id(source_id)).unwrap().clone();
        source.id = id("bound.not.v1");
        source.parents = vec![id(source_id)];
        for port in &mut source.ports {
            port.required_source_types.clear();
        }
        let input = source
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Input)
            .unwrap()
            .clone();
        let output = source
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Output)
            .unwrap()
            .clone();
        source.behavior_bindings = vec![BehaviorBinding::RepeatedSettling {
            behavior_type: definition(true).id,
            inputs: BTreeMap::from([("a".into(), input.name.clone())]),
            outputs: BTreeMap::from([("out".into(), output.name.clone())]),
        }];
        catalog.insert_revision(source.clone()).unwrap();
        let rotation = RotationY::R90;
        let origin = Pos::new(10, 2, 5);
        let position = |p: Pos| {
            let r = rotation.checked_pos(p).unwrap();
            Pos::new(r.x + origin.x, r.y + origin.y, r.z + origin.z)
        };
        let mut world = World::new();
        for b in &source.blocks {
            world.set(
                position(b.position),
                rotation.checked_block(&b.block).unwrap(),
            );
        }
        let lever = position(Pos::new(-1, 0, 0));
        let b = world.place(BlockKind::Lever, lever);
        b.support_offset = Some(rotation.checked_pos(Pos::new(1, 0, 0)).unwrap());
        b.powered = Some(false);
        let assembly = Assembly {
            name: "Rotated existing NOT".into(),
            instances: vec![BlueprintInclusion {
                origin,
                rotation,
                ..include("gate", "bound.not.v1")
            }],
            blocks: world
                .iter()
                .map(|(p, b)| PositionedBlock {
                    position: *p,
                    block: b.clone(),
                })
                .collect(),
            known_regions: world.positions().map(|p| Region::around(p, 2)).collect(),
            connections: vec![],
            boundaries: vec![],
        };
        let context = context(position(input.position), input.kind, lever);
        let report = review(&catalog, &assembly, &context);
        assert_eq!(
            report.status(),
            CheckStatus::Passed,
            "{source_id}: {report:?}"
        );
    }
}

#[test]
fn binding_shape_validation_keeps_all_names_directions_and_old_json_strict() {
    let (catalog, _, _) = fixture();
    let source = catalog.revision(&id("source.v1")).unwrap();
    for mode in 0..6 {
        let mut catalog = catalog.clone();
        let mut bad = source.clone();
        bad.id = id("bad.v1");
        match mode {
            0 => bad.ports[0].direction = PortDirection::Output,
            1 => bad.ports[0].kind = BlueprintPortKind::BlockState,
            2 => bad.behavior_bindings.push(binding(true)),
            3 => {
                bad.behavior_bindings = vec![BehaviorBinding::Autonomous {
                    behavior_type: definition(true).id,
                    output_port: "output".into(),
                }]
            }
            _ => {
                if let BehaviorBinding::RepeatedSettling {
                    inputs, outputs, ..
                } = &mut bad.behavior_bindings[0]
                {
                    if mode == 4 {
                        inputs.clear();
                    } else {
                        outputs.insert("unknown".into(), "output".into());
                    }
                }
            }
        }
        assert!(catalog.insert_revision(bad).is_err(), "mode {mode}");
    }
    for value in [
        serde_json::json!({"behavior_type":"relation.not.v1","output_port":"out","inputs":{},"outputs":{}}),
        serde_json::json!({"behavior_type":"relation.not.v1","inputs":{"a":"in"},"outputs":{"out":"out"},"certificate":"passed"}),
    ] {
        assert!(serde_json::from_value::<BehaviorBinding>(value).is_err());
    }
    let original = serde_json::json!({"behavior_type":"clock.v1","output_port":"out"});
    let parsed: BehaviorBinding = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
}

#[test]
fn multiport_aliases_and_consumer_requirements_share_only_the_complete_binding_proof() {
    let (mut catalog, mut assembly, mut context) = fixture();
    let mut world = World::new();
    let mut ports = vec![];
    context.input_drivers.clear();
    for (name, z) in [("z", 0), ("a", 3)] {
        let input = Pos::new(0, 1, z);
        let output = Pos::new(1, 1, z);
        for x in 0..=1 {
            world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
        }
        world.place(BlockKind::Lever, input).powered = Some(false);
        world.place(BlockKind::RedstoneWire, output);
        ports.push(port(
            &format!("in-{name}"),
            Pos::new(0, 0, z),
            BlueprintPortKind::BlockPower,
            PortDirection::Input,
        ));
        ports.push(port(
            &format!("out-{name}"),
            output,
            BlueprintPortKind::Wire,
            PortDirection::Output,
        ));
        context.input_drivers.push(PhysicalInputDriver {
            port_position: Pos::new(0, 0, z),
            port_kind: BlueprintPortKind::BlockPower,
            lever_position: input,
        });
    }
    let definition = TypeRevision {
        id: type_id("multi.v1"),
        name: "Two independent wires".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["z".into(), "a".into()],
                outputs: vec!["right".into(), "left".into()],
                rows: (0..4)
                    .map(|v| {
                        let a = v & 1 != 0;
                        let b = v & 2 != 0;
                        BooleanRow {
                            inputs: vec![a, b],
                            outputs: vec![b, a],
                        }
                    })
                    .collect(),
            },
        },
    };
    catalog.insert_type(definition.clone()).unwrap();
    let mut wires = catalog.revision(&id("source.v1")).unwrap().clone();
    wires.id = id("wires.v1");
    wires.ports = ports;
    wires.blocks = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    wires.behavior_bindings = vec![BehaviorBinding::RepeatedSettling {
        behavior_type: definition.id.clone(),
        inputs: BTreeMap::from([("z".into(), "in-z".into()), ("a".into(), "in-a".into())]),
        outputs: BTreeMap::from([
            ("right".into(), "out-a".into()),
            ("left".into(), "out-z".into()),
        ]),
    }];
    catalog.insert_revision(wires.clone()).unwrap();
    let mut alias = wires.clone();
    alias.id = id("alias.v1");
    alias.blocks.clear();
    alias.inclusions = vec![include("child", "wires.v1")];
    alias.port_bindings = alias
        .ports
        .iter()
        .map(|p| BlueprintPortBinding {
            name: p.name.clone(),
            port: BlueprintPortRef {
                instance: path("child"),
                port: p.name.clone(),
            },
        })
        .collect();
    catalog.insert_revision(alias.clone()).unwrap();
    let mut consumer = wires.clone();
    consumer.id = id("consumer.v1");
    consumer.blocks.clear();
    consumer.behavior_bindings.clear();
    consumer.ports = vec![port(
        "input",
        Pos::new(1, 1, 0),
        BlueprintPortKind::Wire,
        PortDirection::Input,
    )];
    consumer.ports[0].required_source_types = vec![definition.id];
    catalog.insert_revision(consumer).unwrap();
    assembly.blocks = wires.blocks;
    assembly.instances = vec![
        include("root", "alias.v1"),
        include("consumer", "consumer.v1"),
    ];
    assembly.known_regions = world.positions().map(|p| Region::around(p, 2)).collect();
    assembly.connections = vec![AssemblyConnection {
        source: AssemblyPortRef {
            instance: path("root"),
            port: "out-z".into(),
        },
        sink: AssemblyPortRef {
            instance: path("consumer"),
            port: "input".into(),
        },
        path: vec![Pos::new(1, 1, 0)],
    }];
    let report = review_assembly_in_context(
        &catalog,
        &assembly,
        Some(&context),
        BehaviorBudget {
            max_states: 4,
            ..BehaviorBudget::default()
        },
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    assert_eq!(report.behavior.len(), 3);
    assert!(
        report
            .behavior
            .iter()
            .all(|check| check.detail.contains("\"abstract_states\":4"))
    );
    let mut shared_driver = context.clone();
    shared_driver.input_drivers[1].lever_position = shared_driver.input_drivers[0].lever_position;
    assert_eq!(
        review(&catalog, &assembly, &shared_driver).behavior_status(),
        Some(CheckStatus::Undetermined)
    );
    // Same type and outputs, different input mapping: a previous result must
    // not certify the parent. Consumers also cannot choose an ambiguous map.
    let mut conflicting = alias.clone();
    conflicting.id = id("conflicting.alias.v1");
    if let BehaviorBinding::RepeatedSettling { inputs, .. } = &mut conflicting.behavior_bindings[0]
    {
        inputs.insert("z".into(), "in-a".into());
        inputs.insert("a".into(), "in-z".into());
    }
    catalog.insert_revision(conflicting).unwrap();
    assembly.instances[0].revision = id("conflicting.alias.v1");
    let conflicting = review(&catalog, &assembly, &context);
    assert_eq!(conflicting.status(), CheckStatus::Undetermined);
    assert!(
        conflicting
            .behavior
            .iter()
            .any(|check| check.status == CheckStatus::Passed)
    );
    assert!(conflicting.behavior.iter().any(|check| {
        check
            .detail
            .contains("ambiguous repeated-settling port mappings")
    }));
    // Importing the same ports/type labels without a complete producer binding
    // cannot let a consumer infer or guess the missing relation terminals.
    let mut child = catalog.revision(&id("wires.v1")).unwrap().clone();
    child.id = id("unbound.child.v1");
    child.behavior_bindings.clear();
    catalog.insert_revision(child).unwrap();
    alias.id = id("unbound.alias.v1");
    alias.behavior_bindings.clear();
    alias.inclusions[0].revision = id("unbound.child.v1");
    catalog.insert_revision(alias).unwrap();
    assembly.instances[0].revision = id("unbound.alias.v1");
    let report = review(&catalog, &assembly, &context);
    assert_eq!(report.status(), CheckStatus::Undetermined, "{report:?}");
    assert!(
        report.behavior[0]
            .detail
            .contains("no declared repeated-settling port mapping")
    );
}
