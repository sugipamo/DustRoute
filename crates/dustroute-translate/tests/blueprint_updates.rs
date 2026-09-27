use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_library::{PortDirection, Provenance};
use dustroute_translate::blueprint_update::*;
use dustroute_translate::promotion::{CheckStatus, review_assembly};
use dustroute_translate::{Block, BlockKind, GateKind, Pos, Region, RotationY};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn state(value: &str) -> AssemblyRevisionId {
    AssemblyRevisionId::new(value).unwrap()
}
fn update(value: &str) -> BlueprintUpdateId {
    BlueprintUpdateId::new(value).unwrap()
}
fn path(parts: &[&str]) -> InstancePath {
    parts
        .iter()
        .map(|value| InstanceId::new(*value).unwrap())
        .collect()
}
fn include(name: &str, revision: &str) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision: id(revision),
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}
fn regions() -> Vec<Region> {
    vec![Region::new(Pos::new(-4, -4, -4), Pos::new(10, 4, 4))]
}
fn wrapper(
    catalog: &BlueprintCatalog,
    name: &str,
    realization: &str,
    child_name: &str,
    child_revision: &str,
    previous: Option<&str>,
    shared: bool,
) -> BlueprintRevision {
    let mut parent = catalog.revision(&id(realization)).unwrap().clone();
    parent.id = id(name);
    parent.name = name.into();
    parent.parents = previous.into_iter().map(id).collect();
    parent.initial_layout = Some(BlueprintLayout {
        blocks: parent.blocks.clone(),
        known_regions: regions(),
    });
    parent.blocks.clear();
    parent.inclusions = vec![include(child_name, child_revision)];
    if shared {
        parent.inclusions.push(include("c", "shared.v1"));
    }
    parent.port_bindings = parent
        .ports
        .iter()
        .map(|port| BlueprintPortBinding {
            name: port.name.clone(),
            port: BlueprintPortRef {
                instance: path(&[child_name]),
                port: port.name.clone(),
            },
        })
        .collect();
    parent
}

/// Both parents contain a nested NOT and a shared interpretation of its input
/// block. An additional occurrence outside the parent shares that same block.
fn fixture(air: Pos) -> (BlueprintUpdates, BlueprintUpdateRequest) {
    let mut catalog = builtin_blueprints().clone();
    let shared = BlueprintRevision {
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![],
        id: id("shared.v1"),
        parents: vec![],
        name: "Shared input block and clearance".into(),
        classifications: vec![],
        blocks: vec![
            PositionedBlock {
                position: Pos::default(),
                block: Block::new(BlockKind::Solid),
            },
            PositionedBlock {
                position: air,
                block: Block::new(BlockKind::Air),
            },
        ],
        law: None,
        initial_layout: None,
        inclusions: vec![],
        ports: vec![BlueprintPort {
            name: "tap".into(),
            direction: PortDirection::Output,
            position: Pos::default(),
            kind: BlueprintPortKind::BlockPower,
            facing: None,
            required_source_types: vec![],
        }],
        connections: vec![],
        port_bindings: vec![],
        provenance: Provenance {
            author: "test".into(),
            source_url: None,
            license: None,
            retrieved_on: None,
        },
    };
    catalog.insert_revision(shared).unwrap();
    let inner = wrapper(
        &catalog,
        "inner.v1",
        NOT_TOP_REVISION,
        "b",
        NOT_TOP_REVISION,
        None,
        true,
    );
    catalog.insert_revision(inner).unwrap();
    let parent = wrapper(
        &catalog,
        "parent.v1",
        NOT_TOP_REVISION,
        "inner",
        "inner.v1",
        None,
        false,
    );
    catalog.insert_revision(parent).unwrap();
    let actual = |source: &str, parent: &str| Assembly {
        name: "Nested shared NOT arrangement".into(),
        instances: vec![include("root", parent), include("outside", "shared.v1")],
        blocks: catalog.revision(&id(source)).unwrap().blocks.clone(),
        known_regions: regions(),
        connections: vec![],
        boundaries: ["a", "out"]
            .into_iter()
            .map(|name| AssemblyBoundary {
                name: name.into(),
                port: AssemblyPortRef {
                    instance: path(&["root"]),
                    port: name.into(),
                },
            })
            .collect(),
    };
    let base = AssemblyRevision {
        id: state("state.v1"),
        parents: vec![],
        assembly: actual(NOT_TOP_REVISION, "parent.v1"),
    };
    assert_eq!(
        review_assembly(&catalog, &base.assembly).unwrap().status(),
        CheckStatus::Passed
    );
    let candidate = AssemblyRevision {
        id: state("state.v2"),
        parents: vec![base.id.clone()],
        assembly: actual(NOT_SIDE_REVISION, "parent.v2"),
    };
    let inner2 = wrapper(
        &catalog,
        "inner.v2",
        NOT_SIDE_REVISION,
        "replacement",
        NOT_SIDE_REVISION,
        Some("inner.v1"),
        true,
    );
    let parent2 = wrapper(
        &catalog,
        "parent.v2",
        NOT_SIDE_REVISION,
        "inner",
        "inner.v2",
        Some("parent.v1"),
        false,
    );
    catalog.insert_assembly(base).unwrap();
    let request = BlueprintUpdateRequest {
        behavior_context: None,
        id: update("not-update.1"),
        title: "Use the side-torch NOT".into(),
        description: "Explicit placement and changed decomposition".into(),
        base_state: state("state.v1"),
        base_parent: id("parent.v1"),
        candidate_parent: id("parent.v2"),
        parent_instance: path(&["root"]),
        child_before: path(&["inner", "b"]),
        previous_child: id(NOT_TOP_REVISION),
        child_after: path(&["inner", "replacement"]),
        next_child: id(NOT_SIDE_REVISION),
        revisions: vec![parent2, inner2], // Deliberately parent before dependency.
        candidate_state: candidate,
    };
    (BlueprintUpdates::new(catalog), request)
}

#[test]
fn nested_shared_not_update_roundtrips_reviews_and_adopts_without_changing_history() {
    let (mut updates, request) = fixture(Pos::new(6, 0, 0));
    let old = updates.catalog().to_json().unwrap();
    updates.create(request.clone()).unwrap();
    assert_eq!(updates.catalog().to_json().unwrap(), old);
    assert!(
        updates
            .catalog()
            .revision(&request.candidate_parent)
            .is_none()
    );
    assert!(
        updates
            .catalog()
            .assembly(&request.candidate_state.id)
            .is_none()
    );
    let diff = updates.diff(&request.id).unwrap();
    assert!(
        diff.blocks
            .iter()
            .any(|change| change.position == Pos::new(0, 1, 0)
                && change.state.before.as_ref().unwrap().kind == BlockKind::RedstoneTorch
                && change.state.after.as_ref().unwrap().kind == BlockKind::Air)
    );
    assert!(
        diff.occurrences
            .iter()
            .any(|change| change.instance == path(&["root", "inner", "b"])
                && change.occurrence.after.is_none())
    );
    assert!(diff.occurrences.iter().any(|change| change.instance
        == path(&["root", "inner", "replacement"])
        && change.occurrence.after.as_ref().unwrap().revision == id(NOT_SIDE_REVISION)));
    assert_eq!(diff.candidate_sources, request.revisions);
    assert_eq!(
        updates.validate(&request.id).unwrap().status(),
        CheckStatus::Passed
    );
    assert_eq!(
        updates.proposal(&request.id).unwrap().status(),
        UpdateStatus::Open
    );
    assert_eq!(updates.catalog().to_json().unwrap(), old);
    let disk_path = std::env::temp_dir().join(format!(
        "dustroute-update-roundtrip-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&disk_path, updates.to_json().unwrap()).unwrap();
    let mut loaded =
        BlueprintUpdates::from_json(&std::fs::read_to_string(&disk_path).unwrap()).unwrap();
    std::fs::remove_file(&disk_path).unwrap();
    assert_eq!(loaded.diff(&request.id).unwrap(), diff);
    assert_eq!(loaded.proposal(&request.id).unwrap().events().len(), 1);
    loaded.adopt(&request.id).unwrap();
    let proposal = loaded.proposal(&request.id).unwrap();
    assert_eq!(proposal.status(), UpdateStatus::Adopted);
    assert_eq!(proposal.events().len(), 3); // A fresh review precedes adoption.
    assert!(matches!(
        proposal.events()[1],
        UpdateEvent::Validated { .. }
    ));
    assert_eq!(
        loaded.catalog().assembly(&request.candidate_state.id),
        Some(&request.candidate_state)
    );
    let original = BlueprintCatalog::from_json(&old).unwrap();
    for record in original.revisions() {
        assert_eq!(loaded.catalog().revision(&record.id), Some(record));
    }
    for record in original.assemblies() {
        assert_eq!(loaded.catalog().assembly(&record.id), Some(record));
    }
    assert_eq!(
        loaded.catalog().revision(&id("parent.v2")).unwrap().parents,
        vec![id("parent.v1")]
    );
    // Separate behavioral evidence: the two concrete NOT worlds still invert.
    for (state_id, mut cell) in [
        (state("state.v1"), dustroute_translate::not_top_cell()),
        (state("state.v2"), dustroute_translate::not_cell()),
    ] {
        let assembly = &loaded.catalog().assembly(&state_id).unwrap().assembly;
        cell.world = dustroute_translate::assembly::validate_assembly(loaded.catalog(), assembly)
            .unwrap()
            .into_world();
        assert!(dustroute_translate::verify_cell(GateKind::Not, &cell).valid);
    }
    let mut reloaded = BlueprintUpdates::from_json(&loaded.to_json().unwrap()).unwrap();
    assert_eq!(reloaded.proposal(&request.id), loaded.proposal(&request.id));
    assert_eq!(
        reloaded.review(&request.id).unwrap().status(),
        CheckStatus::Passed
    );
    assert!(matches!(
        reloaded.adopt(&request.id),
        Err(BlueprintUpdateError::ClosedProposal(_))
    ));
    assert!(matches!(
        reloaded.reject(&request.id, "too late"),
        Err(BlueprintUpdateError::ClosedProposal(_))
    ));
}

#[test]
fn parent_pass_child_fail_blocks_adoption_and_rejection_preserves_old_pins_and_proposal() {
    // The top-torch layout leaves this cell empty; the side-torch uses it as support.
    let (mut updates, request) = fixture(Pos::new(2, -1, 0));
    let before = updates.catalog().to_json().unwrap();
    updates.create(request.clone()).unwrap();
    let report = updates.validate(&request.id).unwrap();
    assert_eq!(
        report.occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(
        report.occurrences[&path(&["root", "inner", "replacement"])].status(),
        CheckStatus::Passed
    );
    for failed in [path(&["root", "inner", "c"]), path(&["outside"])] {
        assert_eq!(report.occurrences[&failed].status(), CheckStatus::Failed);
    }
    assert!(matches!(
        updates.adopt(&request.id),
        Err(BlueprintUpdateError::Validation(_))
    ));
    assert_eq!(
        updates.proposal(&request.id).unwrap().status(),
        UpdateStatus::Open
    );
    assert_eq!(updates.catalog().to_json().unwrap(), before);
    updates
        .reject(&request.id, "Keep the old layout and shared clearance")
        .unwrap();
    let mut loaded = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert_eq!(
        loaded.proposal(&request.id).unwrap().status(),
        UpdateStatus::Rejected
    );
    assert_eq!(loaded.proposal(&request.id).unwrap().request(), &request);
    assert_eq!(loaded.catalog().to_json().unwrap(), before);
    assert!(matches!(
        loaded.adopt(&request.id),
        Err(BlueprintUpdateError::ClosedProposal(_))
    ));
}

#[test]
fn undetermined_shared_state_cannot_be_adopted() {
    let (mut updates, mut request) = fixture(Pos::new(6, 0, 0));
    request.candidate_state.assembly.known_regions.clear();
    let before = updates.catalog().to_json().unwrap();
    updates.create(request.clone()).unwrap();
    let report = updates.validate(&request.id).unwrap();
    assert_eq!(
        report.occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(report.status(), CheckStatus::Undetermined);
    assert!(updates.diff(&request.id).unwrap().known_regions.is_some());
    assert!(matches!(
        updates.adopt(&request.id),
        Err(BlueprintUpdateError::Validation(_))
    ));
    assert_eq!(updates.catalog().to_json().unwrap(), before);
}

#[test]
fn forged_saved_success_cannot_substitute_for_fresh_verification() {
    let (mut updates, request) = fixture(Pos::new(2, -1, 0));
    updates.create(request.clone()).unwrap();
    updates.validate(&request.id).unwrap();
    let mut archive: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    let report = &mut archive["proposals"][0]["events"][0]["report"];
    for occurrence in report["occurrences"].as_array_mut().unwrap() {
        for check in occurrence[1]["checks"].as_array_mut().unwrap() {
            check["status"] = "passed".into();
        }
    }
    for check in report["arrangement"].as_array_mut().unwrap() {
        check["status"] = "passed".into();
    }
    let mut loaded = BlueprintUpdates::from_json(&archive.to_string()).unwrap();
    let before = loaded.catalog().to_json().unwrap();
    assert!(matches!(
        loaded.adopt(&request.id),
        Err(BlueprintUpdateError::Validation(_))
    ));
    assert_eq!(loaded.catalog().to_json().unwrap(), before);
    assert_eq!(loaded.proposal(&request.id).unwrap().events().len(), 2);
    assert_eq!(
        loaded.review(&request.id).unwrap().status(),
        CheckStatus::Failed
    );
}

#[test]
fn invalid_selections_and_attempted_rebinding_leave_the_workspace_unchanged() {
    let (mut updates, request) = fixture(Pos::new(6, 0, 0));
    let before = updates.to_json().unwrap();
    let mut wrong = request.clone();
    wrong.previous_child = id(NOT_SIDE_REVISION);
    assert!(updates.create(wrong).is_err());
    let mut wrong = request.clone();
    wrong.child_after = path(&["inner", "c"]);
    assert!(updates.create(wrong).is_err());
    let mut wrong = request.clone();
    wrong.revisions.pop();
    assert!(updates.create(wrong).is_err());
    let mut wrong = request.clone();
    wrong.revisions[0].parents.clear();
    assert!(updates.create(wrong).is_err());
    assert_eq!(updates.to_json().unwrap(), before);
    updates.create(request.clone()).unwrap();
    let before = updates.to_json().unwrap();
    assert!(matches!(
        updates.create(request.clone()),
        Err(BlueprintUpdateError::DuplicateProposal(_))
    ));
    assert!(
        updates
            .append_revision(request.revisions[0].clone())
            .is_err()
    );
    assert!(
        updates
            .append_state(request.candidate_state.clone())
            .is_err()
    );
    assert_eq!(updates.to_json().unwrap(), before);
}

#[test]
fn archives_reject_inconsistent_decisions_and_preserve_unreviewed_proposals() {
    let (mut updates, request) = fixture(Pos::new(6, 0, 0));
    updates.create(request.clone()).unwrap();
    let mut loaded = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert!(loaded.proposal(&request.id).unwrap().events().is_empty());
    let mut archive: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    archive["proposals"][0]["events"] = serde_json::json!([
        {"kind": "adopted", "parent": "parent.v2", "state": "state.v2"}
    ]);
    assert!(BlueprintUpdates::from_json(&archive.to_string()).is_err());
    loaded.reject(&request.id, "Prefer the original").unwrap();
    let mut archive: serde_json::Value = serde_json::from_str(&loaded.to_json().unwrap()).unwrap();
    archive["proposals"][0]["events"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"kind": "rejected", "reason": "again"}));
    assert!(BlueprintUpdates::from_json(&archive.to_string()).is_err());
    let original = updates.catalog().to_json().unwrap();
    // Explicit adoption can request its own review; a saved pass is unnecessary.
    updates.adopt(&request.id).unwrap();
    assert_eq!(updates.proposal(&request.id).unwrap().events().len(), 2);
    assert_ne!(updates.catalog().to_json().unwrap(), original);
}

#[test]
fn publishing_a_new_child_definition_does_not_update_or_adopt_any_parent() {
    let (mut updates, mut request) = fixture(Pos::new(6, 0, 0));
    let old_parent = updates
        .catalog()
        .revision(&id("parent.v1"))
        .unwrap()
        .clone();
    let old_state = updates
        .catalog()
        .assembly(&state("state.v1"))
        .unwrap()
        .clone();
    let mut child = updates
        .catalog()
        .revision(&id(NOT_SIDE_REVISION))
        .unwrap()
        .clone();
    child.id = id("side.v2");
    child.parents = vec![id(NOT_SIDE_REVISION)];
    updates.append_revision(child.clone()).unwrap();
    assert_eq!(
        updates.catalog().revision(&old_parent.id),
        Some(&old_parent)
    );
    assert_eq!(updates.catalog().assembly(&old_state.id), Some(&old_state));
    assert_eq!(updates.proposals().count(), 0);
    request.next_child = child.id.clone();
    request
        .revisions
        .iter_mut()
        .find(|revision| revision.id == id("inner.v2"))
        .unwrap()
        .inclusions[0]
        .revision = child.id.clone();
    updates.create(request.clone()).unwrap();
    assert_eq!(
        updates.validate(&request.id).unwrap().status(),
        CheckStatus::Passed
    );
    assert!(
        updates
            .catalog()
            .revision(&request.candidate_parent)
            .is_none()
    );
    updates.adopt(&request.id).unwrap();
    assert_eq!(
        updates.catalog().revision(&old_parent.id),
        Some(&old_parent)
    );
    assert_eq!(updates.catalog().assembly(&old_state.id), Some(&old_state));
    assert_eq!(updates.catalog().revision(&child.id), Some(&child));
}

#[test]
fn unadopted_static_obligations_require_new_history_schema_and_survive_reload() {
    let (mut updates, mut request) = fixture(Pos::new(6, 0, 0));
    assert!(
        updates
            .to_json()
            .unwrap()
            .contains("dustroute.blueprint-updates.v1")
    );
    let requirement = TypeRevisionId::new("test.known-air.v1").unwrap();
    updates
        .append_type(TypeRevision {
            id: requirement.clone(),
            name: "Explicit air identity".into(),
            contract: TypeContract::BlockKind {
                block_kind: BlockKind::Air,
            },
        })
        .unwrap();
    let parent = request
        .revisions
        .iter_mut()
        .find(|source| source.id == request.candidate_parent)
        .unwrap();
    parent.static_type_bindings.push(StaticTypeBinding {
        type_revision: requirement,
        port: parent.ports[0].name.clone(),
    });
    // No validation event exists yet, and no committed source uses the new field.
    // An older reader must reject the format rather than discard this obligation.
    let original = updates.catalog().to_json().unwrap();
    updates.create(request.clone()).unwrap();
    let archive = updates.to_json().unwrap();
    assert!(archive.contains("dustroute.blueprint-updates.v2"));
    assert!(original.contains("dustroute.blueprint-catalog.v7"));
    assert!(!original.contains("static_type_bindings"));
    assert!(
        BlueprintUpdates::from_json(&archive.replace(
            "dustroute.blueprint-updates.v2",
            "dustroute.blueprint-updates.v1"
        ))
        .is_err()
    );
    let mut restored = BlueprintUpdates::from_json(&archive).unwrap();
    assert_eq!(restored.proposal(&request.id).unwrap().request(), &request);
    assert_eq!(
        restored.validate(&request.id).unwrap().status(),
        CheckStatus::Failed
    );
    assert!(restored.adopt(&request.id).is_err());
    assert_eq!(restored.catalog().to_json().unwrap(), original);
}

#[test]
fn unadopted_law_requirements_survive_reload_and_require_world_evidence() {
    use dustroute_library::builtin_laws::{TORCH_LAW_REVISION, builtin_laws};
    let (mut updates, mut request) = fixture(Pos::new(6, 0, 0));
    for law in builtin_laws().revisions() {
        updates.append_revision(law.clone()).unwrap();
    }
    request
        .revisions
        .iter_mut()
        .find(|source| source.id == request.candidate_parent)
        .unwrap()
        .required_laws = vec![id(TORCH_LAW_REVISION)];
    let original = updates.catalog().to_json().unwrap();
    assert!(!original.contains("required_laws"));
    updates.create(request.clone()).unwrap();
    let archive = updates.to_json().unwrap();
    assert!(archive.contains("dustroute.blueprint-updates.v3"));
    for version in 1..3 {
        assert!(
            BlueprintUpdates::from_json(&archive.replace(
                "dustroute.blueprint-updates.v3",
                &format!("dustroute.blueprint-updates.v{version}"),
            ))
            .is_err()
        );
    }
    let mut restored = BlueprintUpdates::from_json(&archive).unwrap();
    assert_eq!(restored.proposal(&request.id).unwrap().request(), &request);
    assert_eq!(
        restored.validate(&request.id).unwrap().status(),
        CheckStatus::Undetermined
    );
    assert!(restored.adopt(&request.id).is_err());
    assert_eq!(restored.catalog().to_json().unwrap(), original);
}

#[test]
fn unadopted_observation_bindings_cannot_be_hidden_in_an_old_history_schema() {
    use dustroute_library::location_observation::ObservedPort;
    use std::collections::BTreeMap;
    let (mut updates, mut request) = fixture(Pos::new(6, 0, 0));
    let type_id = TypeRevisionId::new("test.explicit-observation.v1").unwrap();
    updates
        .append_type(TypeRevision {
            id: type_id.clone(),
            name: "Declared recurrence".into(),
            contract: TypeContract::Periodic {
                requirement: dustroute_library::behavior_type::Periodic {
                    output: "out".into(),
                },
            },
        })
        .unwrap();
    let parent = request
        .revisions
        .iter_mut()
        .find(|r| r.id == request.candidate_parent)
        .unwrap();
    let output = parent
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .unwrap()
        .name
        .clone();
    parent.behavior_bindings.push(BehaviorBinding::Observed {
        behavior_type: type_id,
        observed_inputs: BTreeMap::new(),
        observed_outputs: BTreeMap::from([("out".into(), ObservedPort::Signal { port: output })]),
    });
    let original = updates.catalog().to_json().unwrap();
    assert!(!original.contains("observed_outputs"));
    updates.create(request.clone()).unwrap();
    let archive = updates.to_json().unwrap();
    assert!(archive.contains("dustroute.blueprint-updates.v4"));
    for version in 1..4 {
        assert!(
            BlueprintUpdates::from_json(&archive.replace(
                "dustroute.blueprint-updates.v4",
                &format!("dustroute.blueprint-updates.v{version}")
            ))
            .is_err()
        );
    }
    let mut restored = BlueprintUpdates::from_json(&archive).unwrap();
    assert_eq!(restored.proposal(&request.id).unwrap().request(), &request);
    assert_eq!(
        restored.validate(&request.id).unwrap().behavior_status(),
        Some(CheckStatus::Undetermined)
    );
    assert!(restored.adopt(&request.id).is_err());
    assert_eq!(restored.catalog().to_json().unwrap(), original);
}
