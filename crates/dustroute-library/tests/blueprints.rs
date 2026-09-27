use std::collections::BTreeSet;

use dustroute_library::blueprint::*;
use dustroute_library::{LogicalSpec, Port, PortDirection, Provenance};
use dustroute_minecraft::{Block, BlockKind, Pos};

fn revision_id(name: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(name).unwrap()
}

fn instance(name: &str) -> InstanceId {
    InstanceId::new(name).unwrap()
}

fn block(x: i32, kind: BlockKind) -> PositionedBlock {
    PositionedBlock {
        position: Pos::new(x, 0, 0),
        block: Block::new(kind),
    }
}

fn revision(name: &str, blocks: Vec<PositionedBlock>) -> BlueprintRevision {
    BlueprintRevision {
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![],
        law: None,
        initial_layout: None,
        id: revision_id(name),
        parents: vec![],
        name: name.into(),
        classifications: vec![],
        blocks,
        inclusions: vec![],
        ports: vec![],
        connections: vec![],
        port_bindings: vec![],
        provenance: Provenance {
            author: "test".into(),
            source_url: None,
            license: None,
            retrieved_on: None,
        },
    }
}

#[test]
fn batch_insertion_rolls_back_missing_dependencies_cycles_and_duplicates() {
    let mut catalog = BlueprintCatalog::default();
    catalog
        .insert_revision(revision("existing.v1", vec![]))
        .unwrap();
    let before = catalog.to_json().unwrap();
    let first = revision("first.v1", vec![]);
    let mut second = revision("second.v1", vec![]);
    second.inclusions.push(include("missing", "absent.v1", 0));
    assert!(
        catalog
            .insert_revisions(vec![first.clone(), second])
            .is_err()
    );
    assert_eq!(catalog.to_json().unwrap(), before);
    let mut cyclic = revision("cyclic.v1", vec![]);
    cyclic.inclusions.push(include("self", "cyclic.v1", 0));
    assert!(
        catalog
            .insert_revisions(vec![first.clone(), cyclic])
            .is_err()
    );
    assert_eq!(catalog.to_json().unwrap(), before);
    assert!(
        catalog
            .insert_revisions(vec![first.clone(), first])
            .is_err()
    );
    assert_eq!(catalog.to_json().unwrap(), before);
}

fn include(name: &str, target: &str, x: i32) -> BlueprintInclusion {
    BlueprintInclusion {
        rotation: Default::default(),
        instance: instance(name),
        revision: revision_id(target),
        origin: Pos::new(x, 0, 0),
    }
}

fn shared_catalog() -> BlueprintCatalog {
    let mut catalog = BlueprintCatalog::default();
    catalog
        .insert_revision(revision(
            "part.v1",
            vec![block(0, BlockKind::Solid), block(1, BlockKind::Solid)],
        ))
        .unwrap();
    let mut parent = revision("parent.v1", vec![]);
    parent.inclusions = vec![include("b", "part.v1", 0), include("c", "part.v1", 1)];
    catalog.insert_revision(parent).unwrap();
    catalog
}

#[test]
fn overlapping_interpretations_share_one_physical_block_and_survive_persistence() {
    let catalog = shared_catalog();
    let original = catalog.expand(&revision_id("parent.v1")).unwrap();
    assert_eq!(original.blocks.len(), 3);
    assert_eq!(
        original.membership[&Pos::new(1, 0, 0)],
        BTreeSet::from([vec![], vec![instance("b")], vec![instance("c")],])
    );
    assert_eq!(
        original.affected_occurrences(&BTreeSet::from([Pos::new(1, 0, 0)])),
        BTreeSet::from([vec![], vec![instance("b")], vec![instance("c")],])
    );
    // Lexicographic archive order deliberately puts the parent before its child.
    let saved = catalog.to_json().unwrap();
    let loaded = BlueprintCatalog::from_json(&saved).unwrap();
    assert_eq!(loaded.expand(&revision_id("parent.v1")).unwrap(), original);
    assert_eq!(loaded.to_json().unwrap(), saved);
}

#[test]
fn same_classification_does_not_require_the_same_children_and_updates_are_immutable() {
    let mut catalog = shared_catalog();
    let type_id = ClassificationRevisionId::new("classification.a.v1").unwrap();
    catalog
        .insert_classification(ClassificationRevision {
            id: type_id.clone(),
            name: "A".into(),
            logical_claim: None,
        })
        .unwrap();
    let mut a1 = catalog.revision(&revision_id("parent.v1")).unwrap().clone();
    a1.id = revision_id("a.v1");
    a1.classifications = vec![type_id.clone()];
    catalog.insert_revision(a1.clone()).unwrap();
    let mut a2 = revision("a.v2", vec![block(0, BlockKind::Solid)]);
    a2.parents = vec![a1.id.clone()];
    a2.classifications = vec![type_id.clone()];
    catalog.insert_revision(a2.clone()).unwrap();
    assert_eq!(catalog.candidates(&type_id).len(), 2);
    assert_eq!(catalog.revision(&a1.id), Some(&a1));
    assert!(catalog.revision(&a2.id).unwrap().inclusions.is_empty());
    a2.id = a1.id.clone();
    assert_eq!(
        catalog.insert_revision(a2),
        Err(BlueprintError::DuplicateRevision(a1.id.clone()))
    );
    assert_eq!(catalog.revision(&a1.id), Some(&a1));
    // A child revision is also a new record, not an update to pinned references.
    let mut child2 = revision("part.v2", vec![block(0, BlockKind::Transparent)]);
    child2.parents = vec![revision_id("part.v1")];
    catalog.insert_revision(child2).unwrap();
    assert_eq!(
        catalog.expand(&a1.id).unwrap().blocks[&Pos::new(1, 0, 0)].kind,
        BlockKind::Solid
    );
}

#[test]
fn conflicting_shared_states_are_reported_atomically_without_choosing_a_completion_policy() {
    let mut catalog = shared_catalog();
    catalog
        .insert_revision(revision(
            "replacement.v1",
            vec![block(0, BlockKind::Solid), block(1, BlockKind::Transparent)],
        ))
        .unwrap();
    let before = catalog.to_json().unwrap();
    let mut changed = catalog.revision(&revision_id("parent.v1")).unwrap().clone();
    changed.id = revision_id("parent.v2");
    changed.parents = vec![revision_id("parent.v1")];
    changed.inclusions[0].revision = revision_id("replacement.v1");
    assert!(matches!(
        catalog.insert_revision(changed),
        Err(BlueprintError::ConflictingBlocks {
            position: Pos { x: 1, y: 0, z: 0 },
            ..
        })
    ));
    assert_eq!(catalog.to_json().unwrap(), before);
}

#[test]
fn explicit_air_is_a_requirement_but_unspecified_space_is_not() {
    let mut catalog = BlueprintCatalog::default();
    catalog
        .insert_revision(revision("empty-space", vec![block(0, BlockKind::Air)]))
        .unwrap();
    let expanded = catalog.expand(&revision_id("empty-space")).unwrap();
    assert!(expanded.proposed_world().get(Pos::default()).is_none());
    assert_eq!(expanded.blocks[&Pos::default()].kind, BlockKind::Air);
    assert!(!expanded.blocks.contains_key(&Pos::new(1, 0, 0)));
    let mut conflict = revision("occupied", vec![block(0, BlockKind::Solid)]);
    conflict.inclusions.push(include("empty", "empty-space", 0));
    assert!(matches!(
        catalog.insert_revision(conflict),
        Err(BlueprintError::ConflictingBlocks { .. })
    ));
}

#[test]
fn archives_reject_unknown_references_and_separate_containment_from_ancestry_cycles() {
    let original: serde_json::Value =
        serde_json::from_str(&shared_catalog().to_json().unwrap()).unwrap();
    let parent_index = original["revisions"]
        .as_array()
        .unwrap()
        .iter()
        .position(|value| value["id"] == "parent.v1")
        .unwrap();
    let mut missing = original.clone();
    missing["revisions"][parent_index]["inclusions"][0]["revision"] = "missing".into();
    assert_eq!(
        BlueprintCatalog::from_json(&missing.to_string()).unwrap_err(),
        BlueprintError::UnknownRevision(revision_id("missing"))
    );
    let mut cycle = original.clone();
    cycle["revisions"][parent_index]["inclusions"][0]["revision"] = "parent.v1".into();
    assert!(matches!(
        BlueprintCatalog::from_json(&cycle.to_string()),
        Err(BlueprintError::ContainmentCycle(_))
    ));
    let mut ancestry = original;
    ancestry["revisions"][parent_index]["parents"] = serde_json::json!(["parent.v1"]);
    assert!(matches!(
        BlueprintCatalog::from_json(&ancestry.to_string()),
        Err(BlueprintError::AncestryCycle(_))
    ));
}

#[test]
fn expansion_checks_overflow_and_bounds_work_even_when_instances_share_all_blocks() {
    let mut catalog = shared_catalog();
    let mut overflowing = revision("overflow", vec![]);
    overflowing
        .inclusions
        .push(include("part", "part.v1", i32::MAX));
    assert_eq!(
        catalog.insert_revision(overflowing),
        Err(BlueprintError::CoordinateOverflow)
    );
    let mut repeated = revision("repeated", vec![]);
    repeated.inclusions = vec![
        include("first", "part.v1", 0),
        include("second", "part.v1", 0),
    ];
    catalog.insert_revision(repeated).unwrap();
    assert_eq!(
        catalog.expand_with_limits(
            &revision_id("repeated"),
            ExpansionLimits {
                maximum_block_claims: 3,
                ..ExpansionLimits::default()
            }
        ),
        Err(BlueprintError::ExpansionLimit("block claims"))
    );
    assert_eq!(
        catalog.expand_with_limits(
            &revision_id("repeated"),
            ExpansionLimits {
                maximum_occurrences: 2,
                ..ExpansionLimits::default()
            }
        ),
        Err(BlueprintError::ExpansionLimit("occurrences"))
    );
    assert_eq!(
        catalog.expand_with_limits(
            &revision_id("repeated"),
            ExpansionLimits {
                maximum_depth: 0,
                ..ExpansionLimits::default()
            }
        ),
        Err(BlueprintError::ExpansionLimit("depth"))
    );
}

#[test]
fn optional_behavior_claims_belong_to_classifications_not_connection_types() {
    let mut catalog = BlueprintCatalog::default();
    let specification = LogicalSpec {
        ports: [
            ("a", PortDirection::Input),
            ("b", PortDirection::Input),
            ("sum", PortDirection::Output),
            ("carry", PortDirection::Output),
        ]
        .into_iter()
        .map(|(name, direction)| Port {
            name: name.into(),
            direction,
            bit_width: 1,
        })
        .collect(),
        truth_table: vec![
            vec![false, false, false, false],
            vec![false, true, true, false],
            vec![true, false, true, false],
            vec![true, true, false, true],
        ],
        stateful: false,
    };
    let id = ClassificationRevisionId::new("half-adder.v1").unwrap();
    let definition = ClassificationRevision {
        id: id.clone(),
        name: "Half adder".into(),
        logical_claim: Some(specification),
    };
    catalog.insert_classification(definition.clone()).unwrap();
    assert_eq!(
        catalog.insert_classification(definition.clone()),
        Err(BlueprintError::DuplicateClassification(id.clone()))
    );
    let mut malformed = definition;
    malformed.id = ClassificationRevisionId::new("broken.v1").unwrap();
    let specification = malformed.logical_claim.as_mut().unwrap();
    specification.truth_table[0] = specification.truth_table[1].clone();
    assert!(matches!(
        catalog.insert_classification(malformed),
        Err(BlueprintError::Invalid(_))
    ));
    assert_eq!(catalog.classification(&id).unwrap().name, "Half adder");
    assert_eq!(catalog.type_revisions().count(), 0);
}

#[test]
fn consumer_requirements_pin_existing_type_revisions_without_certifying_them() {
    let mut catalog = BlueprintCatalog::default();
    let type_id = TypeRevisionId::new("source-block.v1").unwrap();
    let mut consumer = revision("consumer.v1", vec![]);
    consumer.ports.push(BlueprintPort {
        name: "in".into(),
        direction: PortDirection::Input,
        position: Pos::default(),
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![type_id.clone()],
    });
    assert_eq!(
        catalog.insert_revision(consumer.clone()),
        Err(BlueprintError::UnknownType(type_id.clone()))
    );
    catalog
        .insert_type(TypeRevision {
            id: type_id,
            name: "Source block".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![block(0, BlockKind::Solid)],
            },
        })
        .unwrap();
    catalog.insert_revision(consumer.clone()).unwrap();
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&consumer.id), Some(&consumer));
    assert!(loaded.expand(&consumer.id).unwrap().blocks.is_empty());
}

#[test]
fn classification_ids_cannot_be_used_as_connection_types() {
    let mut catalog = BlueprintCatalog::default();
    catalog
        .insert_classification(ClassificationRevision {
            id: ClassificationRevisionId::new("not.v1").unwrap(),
            name: "NOT".into(),
            logical_claim: None,
        })
        .unwrap();
    let mut consumer = revision("consumer.v1", vec![]);
    let required = TypeRevisionId::new("not.v1").unwrap();
    consumer.ports.push(BlueprintPort {
        name: "in".into(),
        direction: PortDirection::Input,
        position: Pos::default(),
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: vec![required.clone()],
    });
    assert_eq!(
        catalog.insert_revision(consumer),
        Err(BlueprintError::UnknownType(required))
    );
    assert!(
        serde_json::from_str::<TypeContract>(r#"{"kind":"boolean_function","specification":{}}"#)
            .is_err()
    );
}

#[test]
fn every_required_pattern_must_match_and_unknown_space_is_not_air_evidence() {
    let mut catalog = BlueprintCatalog::default();
    for (name, blocks) in [
        ("solid.v1", vec![block(0, BlockKind::Solid)]),
        ("empty.v1", vec![block(1, BlockKind::Air)]),
    ] {
        catalog
            .insert_type(TypeRevision {
                id: TypeRevisionId::new(name).unwrap(),
                name: name.into(),
                contract: TypeContract::BlockPattern { blocks },
            })
            .unwrap();
    }
    let source = BlueprintPort {
        name: "physical-out".into(),
        direction: PortDirection::Output,
        position: Pos::new(8, 4, -2),
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    };
    let mut consumer = source.clone();
    consumer.direction = PortDirection::Input;
    consumer.required_source_types = vec![
        TypeRevisionId::new("solid.v1").unwrap(),
        TypeRevisionId::new("empty.v1").unwrap(),
    ];
    assert_eq!(
        catalog.check_source_requirements(&source, &consumer, |position| {
            (position == Pos::default()).then(|| Block::new(BlockKind::Solid))
        }),
        Err(BlueprintError::UnsatisfiedSourceType(
            TypeRevisionId::new("empty.v1").unwrap()
        ))
    );
    catalog
        .check_source_requirements(&source, &consumer, |position| match position.x {
            0 => Some(Block::new(BlockKind::Solid)),
            1 => Some(Block::new(BlockKind::Air)),
            _ => None,
        })
        .unwrap();
    assert!(
        catalog
            .check_source_requirements(&source, &consumer, |_| Some(Block::new(BlockKind::Air)))
            .is_err()
    );
}
