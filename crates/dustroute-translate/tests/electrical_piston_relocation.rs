#[path = "support/runtime_blueprint.rs"]
mod fixture;

use dustroute_library::assembly::AssemblyPortRef;
use dustroute_library::blueprint::InstanceId;
use dustroute_minecraft::{Pos, RotationY};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};

#[test]
fn relocation_preserves_child_references_and_rechecks_real_input_and_terminal_positions() {
    let f = fixture::electrical_fixture(false);
    let mut catalog = f.catalog;
    for source in f.request.revisions.iter().rev() {
        catalog.insert_revision(source.clone()).unwrap();
    }
    let source = &f.request.candidate_state.assembly;
    let original_catalog = catalog.clone();
    let transform = AssemblyTransform {
        source_anchor: Pos::default(),
        target_anchor: Pos::new(-96, 180, 1000),
        rotation: RotationY::R90,
    };
    let (moved, context) = transform.apply(source, &f.context).unwrap();
    let terminal = AssemblyPortRef {
        instance: ["root", "mechanism"]
            .map(|s| InstanceId::new(s).unwrap())
            .to_vec(),
        port: "input".into(),
    };
    let before = source
        .inspect(&catalog)
        .unwrap()
        .resolved_port(&terminal)
        .unwrap()
        .0
        .position;
    let after = moved
        .inspect(&catalog)
        .unwrap()
        .resolved_port(&terminal)
        .unwrap()
        .0
        .position;
    assert_eq!(after, transform.position(before).unwrap());
    assert_eq!(context.input_levers, vec![after]);
    assert_eq!(moved.instances[0].revision, source.instances[0].revision);
    let report = review_assembly_with_context(
        &catalog,
        &moved,
        Some(&context.clone().into()),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    assert_eq!(catalog, original_catalog);
    assert_eq!(catalog.assembly(&f.base.id), Some(&f.base));
    let stale = review_assembly_with_context(
        &catalog,
        &moved,
        Some(&f.context.into()),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_ne!(stale.status(), CheckStatus::Passed);
    let inverse = AssemblyTransform {
        source_anchor: transform.target_anchor,
        target_anchor: transform.source_anchor,
        rotation: transform.rotation.inverse(),
    };
    assert_eq!(inverse.apply(&moved, &context).unwrap().0, *source);
}

#[test]
fn unrepresentable_relocation_is_rejected_before_any_world_is_created() {
    let transform = AssemblyTransform {
        source_anchor: Pos::default(),
        target_anchor: Pos::default(),
        rotation: RotationY::R90,
    };
    assert!(transform.position(Pos::new(0, 0, i32::MIN)).is_err());
    let transform = AssemblyTransform {
        target_anchor: Pos::new(i32::MAX, 0, 0),
        rotation: RotationY::R0,
        ..transform
    };
    assert!(transform.position(Pos::new(1, 0, 0)).is_err());
}
