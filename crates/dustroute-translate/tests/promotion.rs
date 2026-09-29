use dustroute_library::PortDirection;
use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::promotion::*;
use dustroute_translate::{
    cells::RotationY, wire::update_wire_shapes, world::Block, world::BlockKind, world::Pos,
    world::Region, world::World,
};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn instance(value: &str) -> InstanceId {
    InstanceId::new(value).unwrap()
}
fn path(parts: &[&str]) -> InstancePath {
    parts.iter().map(|part| instance(part)).collect()
}
fn records(world: &World) -> Vec<PositionedBlock> {
    world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect()
}
fn line() -> World {
    let mut world = World::new();
    for x in -1..=1 {
        world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
    }
    update_wire_shapes(&mut world);
    world
}
fn source(catalog: &BlueprintCatalog, name: &str, world: &World) -> BlueprintRevision {
    let mut source = catalog.revision(&id(TERMINAL_REVISION)).unwrap().clone();
    source.id = id(name);
    source.name = name.into();
    source.classifications.clear();
    source.ports.clear();
    source.blocks = records(world);
    source
}
fn inclusion(
    name: &str,
    revision: &BlueprintRevisionId,
    rotation: RotationY,
) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: instance(name),
        revision: revision.clone(),
        origin: Pos::default(),
        rotation,
    }
}
fn grouping(name: &str) -> BlueprintGrouping {
    BlueprintGrouping {
        id: id(name),
        name: name.into(),
        classifications: vec![],
        provenance: builtin_blueprints()
            .revision(&id(TERMINAL_REVISION))
            .unwrap()
            .provenance
            .clone(),
    }
}
fn cross() -> (BlueprintCatalog, Assembly, BlueprintRevision) {
    let mut catalog = builtin_blueprints().clone();
    let local = line();
    let child = source(&catalog, "linear-wire.v1", &local);
    catalog.insert_revision(child.clone()).unwrap();
    let mut world = local.clone();
    for (pos, block) in local.iter() {
        world.set(RotationY::R90.pos(*pos), RotationY::R90.block(block));
    }
    update_wire_shapes(&mut world);
    let assembly = Assembly {
        name: "Shared cross".into(),
        instances: vec![
            inclusion("b", &child.id, RotationY::R0),
            inclusion("c", &child.id, RotationY::R90),
        ],
        blocks: records(&world),
        known_regions: vec![Region::new(Pos::new(-2, 0, -2), Pos::new(2, 2, 2))],
        connections: vec![],
        boundaries: vec![],
    };
    (catalog, assembly, child)
}

#[test]
fn wire_to_block_routes_use_actual_arms_through_rotation_reload_and_adoption() {
    use dustroute_translate::assembly::validate_assembly;
    use dustroute_translate::electrical::{DeviceOutputState, solve_instantaneous};
    use dustroute_translate::{world::Facing, world::WireConnection};
    let dust = Pos::new(0, 1, 0);
    let sink = Pos::new(1, 1, 0);
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        for arm in [false, true] {
            for powered in [false, true] {
                let mut catalog = builtin_blueprints().clone();
                let mut world = World::new();
                world.set(Pos::default(), Block::new(BlockKind::Solid));
                world.set(sink, Block::new(BlockKind::Solid));
                if powered {
                    world.set(Pos::new(0, 1, -1), Block::new(BlockKind::RedstoneBlock));
                }
                let wire = world.place(BlockKind::RedstoneWire, dust);
                wire.wire_connections = Some(
                    [
                        (Facing::North, WireConnection::Side),
                        (Facing::South, WireConnection::Side),
                        (
                            Facing::East,
                            if arm {
                                WireConnection::Side
                            } else {
                                WireConnection::None
                            },
                        ),
                        (Facing::West, WireConnection::None),
                    ]
                    .into_iter()
                    .collect(),
                );
                let mut child = source(&catalog, "arm-route.v1", &world);
                child.ports = vec![
                    BlueprintPort {
                        name: "out".into(),
                        direction: PortDirection::Output,
                        position: dust,
                        kind: BlueprintPortKind::Wire,
                        facing: None,
                        required_source_types: vec![],
                    },
                    BlueprintPort {
                        name: "in".into(),
                        direction: PortDirection::Input,
                        position: sink,
                        kind: BlueprintPortKind::BlockPower,
                        facing: Some(Facing::West),
                        required_source_types: vec![],
                    },
                ];
                child.connections = vec![BlueprintConnection {
                    source: BlueprintPortRef {
                        instance: vec![],
                        port: "out".into(),
                    },
                    sink: BlueprintPortRef {
                        instance: vec![],
                        port: "in".into(),
                    },
                    path: vec![dust, sink],
                }];
                catalog.insert_revision(child.clone()).unwrap();
                let assembly = Assembly {
                    name: "arm route".into(),
                    instances: vec![inclusion("child", &child.id, rotation)],
                    blocks: world
                        .iter()
                        .map(|(position, block)| PositionedBlock {
                            position: rotation.pos(*position),
                            block: rotation.block(block),
                        })
                        .collect(),
                    known_regions: vec![Region::new(Pos::new(-3, -3, -3), Pos::new(3, 3, 3))],
                    connections: vec![],
                    boundaries: vec![],
                }
                .with_source_connections(&catalog)
                .unwrap();
                let state = AssemblyRevision {
                    id: AssemblyRevisionId::new("arm-state.v1").unwrap(),
                    parents: vec![],
                    assembly: assembly.clone(),
                };
                catalog.insert_assembly(state.clone()).unwrap();
                let mut loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
                assert_eq!(loaded.assembly(&state.id), Some(&state));
                let saved = loaded.to_json().unwrap();
                let status = if arm {
                    CheckStatus::Passed
                } else {
                    CheckStatus::Failed
                };
                assert_eq!(
                    review_assembly(&loaded, &assembly).unwrap().status(),
                    status
                );
                assert_eq!(validate_assembly(&loaded, &assembly).is_ok(), arm);
                let candidate =
                    PromotionCandidate::prepare(&loaded, &assembly, grouping("arm-parent.v1"))
                        .unwrap();
                let review = candidate.validate(&loaded).unwrap();
                assert_eq!(review.report().status(), status);
                assert_eq!(review.adopt(&mut loaded).is_ok(), arm);
                if !arm {
                    assert_eq!(loaded.to_json().unwrap(), saved);
                }
                assert_eq!(loaded.revision(&child.id), Some(&child));
                assert_eq!(loaded.assembly(&state.id), Some(&state));
                let actual = assembly.inspect(&loaded).unwrap().proposed_world();
                let solved =
                    solve_instantaneous(&actual, &DeviceOutputState::default(), 128).unwrap();
                assert_eq!(
                    solved.signal(rotation.pos(dust)),
                    if powered { 15 } else { 0 }
                );
                assert_eq!(
                    solved.power(rotation.pos(sink)).weak,
                    if powered && arm { 15 } else { 0 }
                );
                // Support directly below remains a weak-power target in both cases.
                assert_eq!(
                    solved.power(Pos::default()).weak,
                    if powered { 15 } else { 0 }
                );
            }
        }
    }
}

#[test]
fn rise_conflicts_and_unknown_clearance_block_shared_child_adoption_without_rewriting_sources() {
    use dustroute_translate::assembly::validate_assembly;
    let mut catalog = builtin_blueprints().clone();
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
    update_wire_shapes(&mut world);
    let child = source(&catalog, "rising-wire.v1", &world);
    catalog.insert_revision(child.clone()).unwrap();
    let mut parent = source(&catalog, "rise-parent.v1", &World::new());
    parent.inclusions = vec![inclusion("child", &child.id, RotationY::R0)];
    catalog.insert_revision(parent.clone()).unwrap();
    let mut assembly = Assembly {
        name: "Shared explicit rise".into(),
        instances: vec![
            inclusion("parent", &parent.id, RotationY::R0),
            inclusion("shared", &child.id, RotationY::R0),
        ],
        blocks: records(&world),
        known_regions: vec![],
        connections: vec![],
        boundaries: vec![],
    };
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Undetermined
    );
    assert!(validate_assembly(&catalog, &assembly).is_err());
    assembly.known_regions = vec![Region::new(Pos::new(-2, -2, -2), Pos::new(3, 4, 2))];
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Passed
    );
    assert!(validate_assembly(&catalog, &assembly).is_ok());

    world.set(Pos::new(0, 2, 0), Block::new(BlockKind::Solid));
    assembly.blocks = records(&world);
    let original = catalog.to_json().unwrap();
    let original_assembly = assembly.clone();
    let report = review_assembly(&catalog, &assembly).unwrap();
    assert_eq!(
        report.occurrences[&path(&["parent"])].status(),
        CheckStatus::Failed
    );
    for name in [path(&["parent", "child"]), path(&["shared"])] {
        assert_eq!(report.occurrences[&name].status(), CheckStatus::Failed);
        assert!(
            report.occurrences[&name]
                .checks
                .iter()
                .any(|check| check.detail.contains("obstructed"))
        );
    }
    assert!(validate_assembly(&catalog, &assembly).is_err());
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("rise-parent.v2")).unwrap();
    assert!(
        candidate
            .validate(&catalog)
            .unwrap()
            .adopt(&mut catalog)
            .is_err()
    );
    assert_eq!(catalog.to_json().unwrap(), original);
    assert_eq!(assembly, original_assembly);
    // Persistence stores the literal conflict as data, never as a passing proof.
    let state = AssemblyRevision {
        id: AssemblyRevisionId::new("conflicting-rise.v1").unwrap(),
        parents: vec![],
        assembly,
    };
    catalog.insert_assembly(state.clone()).unwrap();
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&child.id), Some(&child));
    assert_eq!(loaded.assembly(&state.id), Some(&state));
    assert_eq!(
        review_assembly(&loaded, &state.assembly).unwrap().status(),
        CheckStatus::Failed
    );
}

#[test]
fn shared_wire_promotion_preserves_actual_layout_and_each_child_source_after_reload_and_rotation() {
    let (mut catalog, assembly, child) = cross();
    let before = catalog.to_json().unwrap();
    let before_state = assembly.clone();
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping("cross.v1")).unwrap();
    assert_eq!(
        candidate
            .blueprint()
            .initial_layout
            .as_ref()
            .unwrap()
            .blocks,
        assembly.blocks
    );
    let review = candidate.validate(&catalog).unwrap();
    assert_eq!(review.report().status(), CheckStatus::Passed);
    assert_eq!(catalog.to_json().unwrap(), before); // Validation never adopts.
    let grouped = review.adopt(&mut catalog).unwrap();
    assert_eq!(assembly, before_state);
    assert_eq!(catalog.revision(&child.id), Some(&child));
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    let mut archive: serde_json::Value = serde_json::from_str(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(archive["schema"], "dustroute.blueprint-catalog.v13");
    archive["schema"] = "dustroute.blueprint-catalog.v1".into();
    assert!(BlueprintCatalog::from_json(&archive.to_string()).is_err());
    let expanded = loaded.expand(&id("cross.v1")).unwrap();
    assert_eq!(
        expanded.proposed_world(),
        assembly.inspect(&loaded).unwrap().proposed_world()
    );
    assert_eq!(expanded.blocks.len(), 10);
    let center = Pos::new(0, 1, 0);
    assert_eq!(
        expanded.source_claims[&center][&path(&["b"])],
        *line().get(center).unwrap()
    );
    assert_ne!(
        expanded.source_claims[&center][&path(&["b"])],
        expanded.blocks[&center]
    );
    assert_eq!(
        grouped.inspect(&loaded).unwrap().source_differences().len(),
        2
    );
    let mut rotated = source(&catalog, "rotated-cross.v1", &World::new());
    rotated.inclusions = vec![inclusion("cross", &id("cross.v1"), RotationY::R90)];
    rotated.inclusions[0].origin = Pos::new(10, 2, 20);
    catalog.insert_revision(rotated.clone()).unwrap();
    let rotated = catalog.expand(&rotated.id).unwrap();
    for (pos, block) in &expanded.blocks {
        let local = RotationY::R90.pos(*pos);
        let position = local.offset(10, 2, 20);
        assert_eq!(rotated.blocks[&position], RotationY::R90.block(block));
    }
    assert_eq!(catalog.revision(&child.id), Some(&child));
}

fn typed_arrangement(offset: Pos, expected: Block) -> (BlueprintCatalog, Assembly) {
    let mut catalog = builtin_blueprints().clone();
    let local = line();
    let requirement = TypeRevisionId::new("context.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: requirement.clone(),
            name: "Producer context".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![PositionedBlock {
                    position: offset,
                    block: expected,
                }],
            },
        })
        .unwrap();
    let port = |name: &str, direction, x, requirements| BlueprintPort {
        name: name.into(),
        direction,
        position: Pos::new(x, 1, 0),
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: requirements,
    };
    let mut producer = source(&catalog, "producer.v1", &local);
    producer.ports = vec![port("out", PortDirection::Output, -1, vec![])];
    let mut consumer = source(&catalog, "consumer.v1", &local);
    consumer.ports = vec![port("in", PortDirection::Input, 1, vec![requirement])];
    catalog.insert_revision(producer.clone()).unwrap();
    catalog.insert_revision(consumer.clone()).unwrap();
    let assembly = Assembly {
        name: "Typed shared interpretations".into(),
        instances: vec![
            inclusion("source", &producer.id, RotationY::R0),
            inclusion("b", &consumer.id, RotationY::R0),
            inclusion("c", &consumer.id, RotationY::R0),
        ],
        blocks: records(&local),
        known_regions: vec![Region::new(Pos::new(-2, 0, -2), Pos::new(2, 2, 2))],
        connections: ["b", "c"]
            .into_iter()
            .map(|name| AssemblyConnection {
                source: AssemblyPortRef {
                    instance: path(&["source"]),
                    port: "out".into(),
                },
                sink: AssemblyPortRef {
                    instance: path(&[name]),
                    port: "in".into(),
                },
                path: (-1..=1).map(|x| Pos::new(x, 1, 0)).collect(),
            })
            .collect(),
        boundaries: vec![],
    };
    (catalog, assembly)
}

#[test]
fn passing_parent_cannot_hide_failing_shared_children_and_failed_adoption_is_atomic() {
    let (mut catalog, assembly) = typed_arrangement(Pos::new(0, -1, 0), Block::new(BlockKind::Air));
    let before = catalog.to_json().unwrap();
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("invalid-parent.v1")).unwrap();
    let review = candidate.validate(&catalog).unwrap();
    assert_eq!(
        review.report().occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    for child in ["b", "c"] {
        assert_eq!(
            review.report().occurrences[&path(&["root", child])].status(),
            CheckStatus::Failed
        );
    }
    assert_eq!(review.report().status(), CheckStatus::Failed);
    assert!(matches!(
        review.adopt(&mut catalog),
        Err(PromotionError::Validation(_))
    ));
    assert_eq!(catalog.to_json().unwrap(), before);
    assert_eq!(
        review.candidate().blueprint().inclusions,
        assembly.instances
    );
}

#[test]
fn unknown_is_not_air_and_a_new_candidate_is_required_after_observation_changes() {
    let (mut catalog, mut assembly) =
        typed_arrangement(Pos::new(99, 0, 0), Block::new(BlockKind::Air));
    let first = PromotionCandidate::prepare(&catalog, &assembly, grouping("unknown.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(first.report().status(), CheckStatus::Undetermined);
    let observed = Pos::new(98, 1, 0);
    assembly.known_regions.push(Region::new(observed, observed));
    let second = PromotionCandidate::prepare(&catalog, &assembly, grouping("known.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(second.report().status(), CheckStatus::Passed);
    assert!(matches!(
        first.adopt(&mut catalog),
        Err(PromotionError::Validation(_))
    ));
    let frozen = second.candidate().assembly().clone();
    assembly.blocks.push(PositionedBlock {
        position: observed,
        block: Block::new(BlockKind::Solid),
    });
    let third = PromotionCandidate::prepare(&catalog, &assembly, grouping("occupied.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(third.report().status(), CheckStatus::Failed);
    assert_eq!(second.adopt(&mut catalog).unwrap(), frozen);
}

#[test]
fn nested_shared_children_are_checked_in_current_context_without_mutating_prior_parent() {
    let expected = line().get(Pos::new(-1, 1, 0)).unwrap().clone();
    let (mut catalog, assembly) = typed_arrangement(Pos::default(), expected);
    let first = PromotionCandidate::prepare(&catalog, &assembly, grouping("parent.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(first.report().status(), CheckStatus::Passed);
    let mut grouped = first.adopt(&mut catalog).unwrap();
    let before = catalog.to_json().unwrap();
    grouped
        .blocks
        .iter_mut()
        .find(|record| record.position == Pos::new(-1, 1, 0))
        .unwrap()
        .block
        .power_level = Some(7);
    let review = PromotionCandidate::prepare(&catalog, &grouped, grouping("outer.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(
        review.report().occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "root"])].status(),
        CheckStatus::Passed
    );
    for child in ["b", "c"] {
        assert_eq!(
            review.report().occurrences[&path(&["root", "root", child])].status(),
            CheckStatus::Failed
        );
    }
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), before);
}

#[test]
fn reviewed_dependencies_cannot_be_rebound_in_another_catalog() {
    let (catalog, assembly) = typed_arrangement(Pos::new(0, -1, 0), Block::new(BlockKind::Solid));
    let review = PromotionCandidate::prepare(&catalog, &assembly, grouping("parent.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(review.report().status(), CheckStatus::Passed);
    let mut archive: serde_json::Value = serde_json::from_str(&catalog.to_json().unwrap()).unwrap();
    let definition = archive["types"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["id"] == "context.v1")
        .unwrap();
    definition["name"] = "Different definition under the same ID".into();
    let mut other = BlueprintCatalog::from_json(&archive.to_string()).unwrap();
    let before = other.to_json().unwrap();
    assert!(matches!(
        review.adopt(&mut other),
        Err(PromotionError::ChangedDependency(_))
    ));
    assert_eq!(other.to_json().unwrap(), before);
}

#[test]
fn compiled_half_adder_passes_initial_checks_without_certifying_its_behavior() {
    let mut catalog = builtin_blueprints().clone();
    let compiled = dustroute_translate::compiler::BaselineCompiler::new(Default::default())
        .compile(&dustroute_translate::circuits::half_adder())
        .unwrap();
    let review = PromotionCandidate::prepare(
        &catalog,
        compiled.assembly.as_ref().unwrap(),
        grouping("adder.v1"),
    )
    .unwrap()
    .validate(&catalog)
    .unwrap();
    assert_eq!(
        review.report().status(),
        CheckStatus::Passed,
        "{:#?}",
        review.report()
    );
    let grouped = review.adopt(&mut catalog).unwrap();
    assert_eq!(
        dustroute_translate::assembly::validate_assembly(&catalog, &grouped).unwrap(),
        compiled.world
    );
    assert_eq!(
        catalog.expand(&id("adder.v1")).unwrap().proposed_world(),
        compiled.world.into_world()
    );
}

#[test]
fn invalid_and_unknown_supports_block_adoption_without_repairing_shared_geometry() {
    let (mut catalog, mut assembly, _) = cross();
    assembly
        .blocks
        .retain(|record| record.position != Pos::default());
    let before = catalog.to_json().unwrap();
    let failed = PromotionCandidate::prepare(&catalog, &assembly, grouping("unsupported.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(failed.report().status(), CheckStatus::Failed);
    for name in ["b", "c"] {
        assert_eq!(
            failed.report().occurrences[&path(&["root", name])].status(),
            CheckStatus::Failed
        );
    }
    assert!(failed.adopt(&mut catalog).is_err());
    assembly.known_regions.clear();
    let unknown = PromotionCandidate::prepare(&catalog, &assembly, grouping("unknown-support.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(unknown.report().status(), CheckStatus::Undetermined);
    assert!(unknown.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), before);
}

#[test]
fn a_broken_internal_route_fails_its_declaring_parent_even_when_all_ports_exist() {
    let (mut catalog, mut assembly) =
        typed_arrangement(Pos::new(0, -1, 0), Block::new(BlockKind::Solid));
    assembly
        .blocks
        .retain(|record| record.position != Pos::new(0, 1, 0));
    let before = catalog.to_json().unwrap();
    let review = PromotionCandidate::prepare(&catalog, &assembly, grouping("broken-route.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    let parent = &review.report().occurrences[&path(&["root"])];
    assert!(
        parent
            .checks
            .iter()
            .any(|check| check.kind == CheckKind::SourceConnection
                && check.status == CheckStatus::Failed)
    );
    for name in ["source", "b", "c"] {
        assert!(
            review.report().occurrences[&path(&["root", name])]
                .checks
                .iter()
                .filter(|check| check.kind == CheckKind::Port)
                .all(|check| check.status == CheckStatus::Passed)
        );
    }
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), before);
}

#[test]
fn captured_parent_state_cannot_discard_a_childs_explicit_air_requirement() {
    let (mut catalog, mut assembly, mut child) = cross();
    child.id = id("child-needing-air.v1");
    let position = Pos::new(5, 0, 0);
    child.blocks.push(PositionedBlock {
        position,
        block: Block::new(BlockKind::Air),
    });
    catalog.insert_revision(child.clone()).unwrap();
    assembly.instances[0].revision = child.id.clone();
    assembly.blocks.push(PositionedBlock {
        position,
        block: Block::new(BlockKind::Solid),
    });
    let before = catalog.to_json().unwrap();
    let review = PromotionCandidate::prepare(&catalog, &assembly, grouping("occupied-air.v1"))
        .unwrap()
        .validate(&catalog)
        .unwrap();
    assert_eq!(
        review.report().occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "b"])].status(),
        CheckStatus::Failed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "c"])].status(),
        CheckStatus::Passed
    );
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), before);
    // Importing unverified data cannot conceal the same child requirement from
    // a legacy geometry adapter that cannot carry it through validation.
    let mut drafts = catalog.clone();
    drafts
        .insert_revision(review.candidate().blueprint().clone())
        .unwrap();
    assert!(
        dustroute_translate::blueprint::blueprint_cell(&drafts, &id("occupied-air.v1")).is_err()
    );
}
