use std::collections::BTreeSet;

use dustroute_library::behavior_type::PhysicalBehaviorContext;
use dustroute_library::blueprint::{BlueprintCatalog, BlueprintRevisionId};
use dustroute_library::builtin_laws::{builtin_laws, dust_law_revision};
use dustroute_library::execution_context::{check_law_requirements, resolve_law_references};
use dustroute_minecraft::execution_context::{
    InitializationPolicy, LawRole, WorldExecutionContext, WorldExecutionProfile,
};
use dustroute_minecraft::law::{Expr, Instruction};

#[test]
fn new_piston_context_is_unified_but_saved_records_still_require_their_profile() {
    use dustroute_library::runtime_behavior::{RuntimeBehaviorContext, RuntimeBehaviorProfile};
    use dustroute_minecraft::{Pos, Region};
    let context = RuntimeBehaviorContext::fresh_pistons(
        Region::new(Pos::new(-6, -2, -5), Pos::new(22, 12, 5)),
        vec![Pos::new(-3, 1, 0)],
    );
    assert_eq!(
        context.profile,
        RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV8
    );
    resolve_law_references(builtin_laws(), &context.execution_context()).unwrap();
    let saved = serde_json::to_value(&context).unwrap();
    let mut previous = saved.clone();
    previous["profile"] = "dustroute.piston-electrical-root-exploration.v7".into();
    assert!(serde_json::from_value::<RuntimeBehaviorContext>(previous).is_err());
    let world = context.execution_context();
    assert_eq!(
        world.device_program_revision(),
        Some(dustroute_minecraft::device_program::REVISION)
    );
    let mut previous_world = serde_json::to_value(&world).unwrap();
    previous_world["profile"] = "dustroute.piston-electrical-callbacks.java-1-21-11.v7".into();
    assert!(serde_json::from_value::<WorldExecutionContext>(previous_world).is_err());
    assert_eq!(
        serde_json::from_value::<RuntimeBehaviorContext>(saved.clone()).unwrap(),
        context
    );
    let mut missing = saved;
    missing.as_object_mut().unwrap().remove("profile");
    assert!(serde_json::from_value::<RuntimeBehaviorContext>(missing).is_err());
}

#[test]
fn old_proof_context_roundtrips_without_rewriting_its_wire_format_or_pins() {
    let saved = serde_json::json!({
        "profile": "dustroute.dust-torch-synchronous-game-tick.v1",
        "initial_condition": "fresh_construction",
        "dust_law": "dustroute.law.dust-strength.v1",
        "torch_law": "dustroute.law.torch.java-1-21-11.v1",
        "max_electrical_iterations": 64
    });
    let context: PhysicalBehaviorContext = serde_json::from_value(saved.clone()).unwrap();
    let normalized = context.execution_context();
    normalized.validate().unwrap();
    assert_eq!(normalized.max_electrical_iterations, Some(64));
    assert_eq!(
        normalized.initialization,
        InitializationPolicy::FreshTorchConstruction
    );
    assert_eq!(
        normalized.laws.values().cloned().collect::<BTreeSet<_>>(),
        context
            .law_revisions()
            .into_iter()
            .map(ToString::to_string)
            .collect()
    );
    resolve_law_references(builtin_laws(), &normalized).unwrap();
    assert_eq!(serde_json::to_value(&context).unwrap(), saved);
    let mut changed = context.clone();
    changed.max_electrical_iterations += 1;
    assert!(!context.same_execution_assumptions(&changed));
    changed = context.clone();
    changed
        .input_drivers
        .push(dustroute_library::behavior_type::PhysicalInputDriver {
            port_position: dustroute_minecraft::Pos::new(1, 0, 0),
            port_kind: dustroute_library::blueprint::BlueprintPortKind::BlockPower,
            lever_position: dustroute_minecraft::Pos::new(2, 0, 0),
        });
    assert!(context.same_execution_assumptions(&changed));
    changed = context.clone();
    changed.torch_law = BlueprintRevisionId::new("alternative.v1").unwrap();
    assert!(!context.same_execution_assumptions(&changed));
}

#[test]
fn profiles_select_distinct_complete_law_sets_without_extending_proof_capabilities() {
    use WorldExecutionProfile::*;
    for (profile, size) in [
        (DustTorchSynchronousGameTickV1, 6),
        (DustSingleTorchBlockEffectsV1, 6),
        (RedstoneCompatibilityBoundaryV1, 10),
        (BoundedRedstoneEventsV1, 11),
        (UnifiedPistonElectricalCallbacksJava12111V8, 14),
    ] {
        let context = WorldExecutionContext::for_profile(profile);
        let saved = builtin_laws().to_json().unwrap();
        let selected = resolve_law_references(builtin_laws(), &context).unwrap();
        assert_eq!(selected.len(), size);
        let required: Vec<_> = selected.values().map(|r| r.id.clone()).collect();
        check_law_requirements(builtin_laws(), &context, &required).unwrap();
        let json = serde_json::to_string(&context).unwrap();
        assert_eq!(
            serde_json::from_str::<WorldExecutionContext>(&json).unwrap(),
            context
        );
        assert_eq!(builtin_laws().to_json().unwrap(), saved);
    }
    let proof = WorldExecutionContext::for_profile(DustTorchSynchronousGameTickV1);
    let native = WorldExecutionContext::for_profile(BoundedRedstoneEventsV1);
    let piston = BlueprintRevisionId::new(native.laws[&LawRole::PistonMotion].clone()).unwrap();
    assert!(
        check_law_requirements(builtin_laws(), &proof, &[piston])
            .unwrap_err()
            .contains("not selected")
    );
}

#[test]
fn electrical_context_roundtrips_and_pins_its_connection_program() {
    use dustroute_library::runtime_behavior::{RuntimeBehaviorContext, RuntimeBehaviorProfile};
    use dustroute_minecraft::{Pos, Region};
    let context = RuntimeBehaviorContext {
        profile: RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV8,
        initial_condition:
            dustroute_library::behavior_type::BehaviorInitialCondition::FreshConstruction,
        known_region: Region::new(Pos::new(-4, -4, -4), Pos::new(20, 10, 4)),
        input_levers: vec![Pos::new(0, 4, -1)],
        root_limits: Default::default(),
    };
    let saved = serde_json::to_string(&context).unwrap();
    assert!(saved.contains("dustroute.piston-electrical-root-exploration.v8"));
    let restored: RuntimeBehaviorContext = serde_json::from_str(&saved).unwrap();
    assert_eq!(context, restored);
    let world = restored.execution_context();
    assert_eq!(
        world.synchronous_runtime_profile(),
        Some(dustroute_minecraft::time::runtime::PROFILE)
    );
    assert_eq!(
        world.laws[&LawRole::PistonConnection],
        dustroute_minecraft::piston_electrical_law::LAW_IDS[2]
    );
    let mut archived = BlueprintCatalog::default();
    for law in resolve_law_references(builtin_laws(), &world)
        .unwrap()
        .values()
    {
        archived.insert_revision((*law).clone()).unwrap();
    }
    let loaded = BlueprintCatalog::from_json(&archived.to_json().unwrap()).unwrap();
    assert_eq!(
        resolve_law_references(&loaded, &world).unwrap(),
        resolve_law_references(builtin_laws(), &world).unwrap()
    );
    let id =
        BlueprintRevisionId::new(dustroute_minecraft::piston_electrical_law::LAW_IDS[2]).unwrap();
    let mut changed = loaded.revision(&id).unwrap().clone();
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "accept".into(),
            value: Expr::Constant { value: 1 },
        });
    let mut conflict = BlueprintCatalog::default();
    conflict.insert_revision(changed).unwrap();
    assert!(
        resolve_law_references(&conflict, &world)
            .unwrap_err()
            .contains("immutable law")
    );
    let mut wrong_pin = world.clone();
    wrong_pin.laws.insert(
        LawRole::PistonConnection,
        dustroute_minecraft::piston_law::PISTON_LAW_IDS[2].into(),
    );
    assert!(wrong_pin.validate().is_err());
    assert!(
        check_law_requirements(
            &loaded,
            &world,
            &[
                BlueprintRevisionId::new("dustroute.law.piston.vertical-control.java-1-21-11.v1")
                    .unwrap()
            ]
        )
        .is_err()
    );
}

#[test]
fn fixed_adapters_reject_a_changed_body_even_when_the_revision_id_matches() {
    use WorldExecutionProfile::*;
    for profile in [
        RedstoneCompatibilityBoundaryV1,
        BoundedRedstoneEventsV1,
        UnifiedPistonElectricalCallbacksJava12111V8,
    ] {
        let context = WorldExecutionContext::for_profile(profile);
        let mut conflict = dust_law_revision().clone();
        conflict
            .law
            .as_mut()
            .unwrap()
            .handlers
            .get_mut("evaluate")
            .unwrap()
            .push(Instruction::Set {
                register: "strength".into(),
                value: Expr::Constant { value: 0 },
            });
        let mut catalog = BlueprintCatalog::default();
        catalog.insert_revision(conflict).unwrap();
        assert!(
            resolve_law_references(&catalog, &context)
                .unwrap_err()
                .contains("immutable law")
        );
        // Correct selection alone is not executable-adapter approval. This
        // distinction keeps unsupported world evaluation undetermined in review.
        check_law_requirements(&catalog, &context, &[dust_law_revision().id.clone()]).unwrap();
        let mut changed = context;
        changed
            .laws
            .insert(LawRole::DustStrength, "alternative.v1".into());
        assert!(changed.validate().is_err());
    }
}

#[test]
fn proof_custom_sources_require_selection_of_transitive_dependencies() {
    let mut dust = dust_law_revision().clone();
    dust.id = BlueprintRevisionId::new("alternative-dust.v1").unwrap();
    let missing = BlueprintRevisionId::new("unselected.v1").unwrap();
    let mut dependency = dust_law_revision().clone();
    dependency.id = missing.clone();
    dust.required_laws = vec![missing];
    let mut catalog = builtin_laws().clone();
    catalog.insert_revision(dependency).unwrap();
    catalog.insert_revision(dust.clone()).unwrap();
    let mut context =
        WorldExecutionContext::for_profile(WorldExecutionProfile::DustTorchSynchronousGameTickV1);
    context
        .laws
        .insert(LawRole::DustStrength, dust.id.to_string());
    assert!(
        resolve_law_references(&catalog, &context)
            .unwrap_err()
            .contains("not selected")
    );
    assert!(check_law_requirements(&catalog, &context, &[dust.id]).is_err());
}

#[test]
fn retired_piston_contexts_are_rejected_instead_of_reinterpreted() {
    use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
    use dustroute_minecraft::{Pos, Region};
    let context = RuntimeBehaviorContext::fresh_pistons(
        Region::new(Pos::new(-6, -2, -5), Pos::new(22, 12, 5)),
        vec![],
    );
    for profile in [
        "dustroute.horizontal-piston-root-exploration.v1",
        "dustroute.vertical-piston-root-exploration.v1",
        "dustroute.piston-direct-root-exploration.v1",
        "dustroute.piston-electrical-root-exploration.v1",
        "dustroute.piston-electrical-root-exploration.v2",
        "dustroute.piston-electrical-root-exploration.v3",
        "dustroute.piston-electrical-root-exploration.v4",
        "dustroute.piston-electrical-root-exploration.v5",
    ] {
        let mut saved = serde_json::to_value(&context).unwrap();
        saved["profile"] = serde_json::json!(profile);
        assert!(
            serde_json::from_value::<dustroute_library::behavior_context::BehaviorReviewContext>(
                saved.clone()
            )
            .is_err()
        );
        let error = serde_json::from_value::<RuntimeBehaviorContext>(saved).unwrap_err();
        assert!(error.to_string().contains(profile), "{error}");
    }
    for profile in [
        "dustroute.horizontal-piston-callbacks.java-1-21-11.v1",
        "dustroute.vertical-piston-callbacks.java-1-21-11.v1",
        "dustroute.piston-direct-callbacks.java-1-21-11.v1",
        "dustroute.piston-electrical-callbacks.java-1-21-11.v1",
        "dustroute.piston-electrical-callbacks.java-1-21-11.v2",
        "dustroute.piston-electrical-callbacks.java-1-21-11.v3",
        "dustroute.piston-electrical-callbacks.java-1-21-11.v4",
        "dustroute.piston-electrical-callbacks.java-1-21-11.v5",
    ] {
        let mut saved = serde_json::to_value(context.execution_context()).unwrap();
        saved["profile"] = serde_json::json!(profile);
        assert!(serde_json::from_value::<WorldExecutionContext>(saved).is_err());
    }
}
