#[path = "support/catalog_fixture.rs"]
mod catalog_fixture;
use catalog_fixture::FixtureJson;
use dustroute_library::PortDirection;
use dustroute_library::behavior_type::Periodic;
use dustroute_library::blueprint::*;
use dustroute_minecraft::{Block, BlockKind, Pos};

#[test]
fn periodic_types_round_trip_but_cannot_be_certified_from_a_snapshot() {
    let mut catalog = BlueprintCatalog::default();
    let id = TypeRevisionId::new("clock.type.v1").unwrap();
    let definition = TypeRevision {
        id: id.clone(),
        name: "Autonomous waveform".into(),
        contract: TypeContract::Periodic {
            requirement: Periodic {
                output: "out".into(),
            },
        },
    };
    catalog.insert_type(definition.clone()).unwrap();
    assert!(catalog.insert_type(definition.clone()).is_err());
    let json = catalog.fixture_json().unwrap();
    assert!(json.contains("dustroute.blueprint-catalog.v13"));
    let loaded = catalog_fixture::catalog(&json).unwrap();
    assert_eq!(loaded.type_revision(&id), Some(&definition));
    for old in ["v1", "v2", "v3"] {
        assert!(
            catalog_fixture::catalog(&json.replace("catalog.v13", &format!("catalog.{old}")))
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
    sink.required_source_types.push(id.clone());
    assert_eq!(
        loaded.check_source_requirements(&source, &sink, |_| Some(Block::new(
            BlockKind::RedstoneWire
        ))),
        Err(BlueprintError::BehavioralEvidenceRequired(id))
    );
    let mut invalid = definition;
    invalid.id = TypeRevisionId::new("invalid.v1").unwrap();
    invalid.contract = TypeContract::Periodic {
        requirement: Periodic { output: " ".into() },
    };
    assert!(catalog.insert_type(invalid).is_err());
}
