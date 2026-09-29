use dustroute_library::PortDirection;
use dustroute_library::behavior_type::FiniteBurst;
use dustroute_library::blueprint::*;
use dustroute_minecraft::{Block, BlockKind, Pos};

#[test]
fn finite_burst_round_trips_in_v5_and_snapshot_evidence_cannot_certify_it() {
    let mut catalog = BlueprintCatalog::default();
    let id = TypeRevisionId::new("burst.type.v1").unwrap();
    let definition = TypeRevision {
        id: id.clone(),
        name: "Finite output activity".into(),
        contract: TypeContract::FiniteBurst {
            requirement: FiniteBurst {
                output: "out".into(),
            },
        },
    };
    catalog.insert_type(definition.clone()).unwrap();
    assert!(catalog.insert_type(definition.clone()).is_err());
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    assert_eq!(
        BlueprintCatalog::from_json(&saved)
            .unwrap()
            .type_revision(&id),
        Some(&definition)
    );
    for version in ["v1", "v2", "v3", "v4"] {
        assert!(
            BlueprintCatalog::from_json(
                &saved.replace("catalog.v13", &format!("catalog.{version}"))
            )
            .is_err()
        );
    }
    let source = BlueprintPort {
        name: "out".into(),
        direction: PortDirection::Output,
        position: Pos::default(),
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: vec![],
    };
    let mut sink = source.clone();
    sink.direction = PortDirection::Input;
    sink.required_source_types = vec![id.clone()];
    assert_eq!(
        catalog.check_source_requirements(&source, &sink, |_| Some(Block::new(
            BlockKind::RedstoneWire
        ))),
        Err(BlueprintError::BehavioralEvidenceRequired(id))
    );
    let mut invalid = definition;
    invalid.id = TypeRevisionId::new("invalid.burst.v1").unwrap();
    invalid.contract = TypeContract::FiniteBurst {
        requirement: FiniteBurst { output: " ".into() },
    };
    assert!(catalog.insert_type(invalid).is_err());
    assert!(serde_json::from_str::<FiniteBurst>(r#"{"output":"out","restartable":true}"#).is_err());
}
