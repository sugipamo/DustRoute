#[allow(dead_code)]
#[path = "support/reference_door_blueprint.rs"]
mod fixture;

use dustroute_library::blueprint::{BehaviorBinding, InstanceId};
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::promotion::CheckStatus;
use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
use dustroute_translate::runtime_review::review_assembly_in_runtime_context;
use dustroute_translate::{BlockKind, Pos, RotationY};

fn path() -> Vec<InstanceId> {
    vec![
        InstanceId::new("root").unwrap(),
        InstanceId::new("mechanism").unwrap(),
    ]
}

#[test]
fn ordinary_door_is_freshly_adopted_and_reverified_after_archive_restart() {
    let f = fixture::ordinary_fixture();
    let source = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let saved = updates.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v11"));
    let mut restored = BlueprintUpdates::from_json(&saved).unwrap();
    restored.adopt(&f.request.id).unwrap();
    assert_eq!(
        updates.catalog(),
        &source,
        "original isolated archive unchanged"
    );
    let restored = BlueprintUpdates::from_json(&restored.to_json().unwrap()).unwrap();
    assert_eq!(restored.catalog().assembly(&f.base.id), Some(&f.base));
    let candidate = restored
        .catalog()
        .assembly(&f.request.candidate_state.id)
        .unwrap();
    assert_eq!(candidate.assembly.blocks, f.base.assembly.blocks);
    let report = review_assembly_in_runtime_context(
        restored.catalog(),
        &candidate.assembly,
        &f.context,
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    assert_eq!(report.behavior.len(), 1);
    assert!(report.behavior[0].report.graph_closed);
}

#[test]
fn aperture_binding_resolves_after_relocation_and_all_horizontal_rotations() {
    let f = fixture::ordinary_fixture();
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    let binding = &catalog
        .revision(&f.request.next_child)
        .unwrap()
        .behavior_bindings[0];
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let (assembly, context) = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor: Pos::new(60_000, 180, 1000),
            rotation,
        }
        .apply(&f.request.candidate_state.assembly, &f.context)
        .unwrap();
        let model = RuntimeBehaviorModel::from_fresh_assembly(
            &catalog,
            &assembly,
            &path(),
            binding,
            &context,
        )
        .unwrap();
        assert_eq!(
            model.outputs(&model.initial_state().unwrap()).unwrap(),
            (0..9).flat_map(|_| [true, false]).collect::<Vec<_>>()
        );
    }
}

#[test]
fn arbitrary_cells_predicates_and_input_sources_cannot_masquerade_as_a_door() {
    for invalid in 0..4 {
        let mut f = fixture::ordinary_fixture();
        let mechanism = f
            .request
            .revisions
            .iter_mut()
            .find(|r| r.id == f.request.next_child)
            .unwrap();
        if invalid == 0 {
            mechanism
                .ports
                .iter_mut()
                .find(|p| p.name == "aperture_2_2")
                .unwrap()
                .position
                .z += 1;
        } else {
            let BehaviorBinding::Observed {
                observed_inputs,
                observed_outputs,
                ..
            } = &mut mechanism.behavior_bindings[0]
            else {
                unreachable!()
            };
            if invalid == 3 {
                let ObservedPort::Location { predicate, .. } =
                    observed_inputs.get_mut("closed_command").unwrap()
                else {
                    unreachable!()
                };
                *predicate = LocationPredicate::Present;
            } else {
                let ObservedPort::Location { predicate, port } =
                    observed_outputs.get_mut("aperture_2_2_solid").unwrap()
                else {
                    unreachable!()
                };
                if invalid == 1 {
                    *predicate = LocationPredicate::BlockKind {
                        block_kind: BlockKind::PistonHead,
                    };
                }
                if invalid == 2 {
                    *port = "aperture_0_0".into();
                }
            }
        }
        let mut catalog = f.catalog;
        catalog.insert_revisions(f.request.revisions).unwrap();
        let binding = &catalog
            .revision(&f.request.next_child)
            .unwrap()
            .behavior_bindings[0];
        assert!(
            RuntimeBehaviorModel::from_fresh_assembly(
                &catalog,
                &f.request.candidate_state.assembly,
                &path(),
                binding,
                &f.context
            )
            .is_err(),
            "invalid binding {invalid}"
        );
    }
}

#[test]
fn inverted_control_polarity_is_read_from_binding_and_detects_wrong_initial_state() {
    let mut f = fixture::ordinary_fixture();
    let mechanism = f
        .request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap();
    let BehaviorBinding::Observed {
        observed_inputs, ..
    } = &mut mechanism.behavior_bindings[0]
    else {
        unreachable!()
    };
    let ObservedPort::Location { predicate, .. } =
        observed_inputs.get_mut("closed_command").unwrap()
    else {
        unreachable!()
    };
    *predicate = LocationPredicate::Powered {
        block_kind: BlockKind::Lever,
        powered: false,
    };
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &f.request.candidate_state.assembly,
        &path(),
        &catalog
            .revision(&f.request.next_child)
            .unwrap()
            .behavior_bindings[0],
        &f.context,
    )
    .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Failed, "{report:?}");
    assert!(report.detail.contains("initial aperture"));
}

#[test]
fn successful_door_behavior_does_not_discard_a_childs_retained_requirement() {
    use dustroute_library::blueprint::{
        StaticTypeBinding, TypeContract, TypeRevision, TypeRevisionId,
    };
    let mut f = fixture::ordinary_fixture();
    let invariant = TypeRevision {
        id: TypeRevisionId::new("test.aperture-always-air.v1").unwrap(),
        name: "Conflicting retained child requirement".into(),
        contract: TypeContract::BlockKind {
            block_kind: BlockKind::Air,
        },
    };
    f.request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap()
        .static_type_bindings
        .push(StaticTypeBinding {
            port: "aperture_0_0".into(),
            type_revision: invariant.id.clone(),
        });
    f.catalog.insert_type(invariant).unwrap();
    f.catalog.insert_revisions(f.request.revisions).unwrap();
    let report = review_assembly_in_runtime_context(
        &f.catalog,
        &f.request.candidate_state.assembly,
        &f.context,
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.behavior[0].report.status, CheckStatus::Passed);
    assert_eq!(report.status(), CheckStatus::Failed);
}
