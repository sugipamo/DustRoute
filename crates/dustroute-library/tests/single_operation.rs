use dustroute_library::behavior_type::SingleOperation;
use dustroute_library::blueprint::{BlueprintCatalog, TypeContract, TypeRevision, TypeRevisionId};

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.single-operation.v1").unwrap(),
        name: "One irreversible operation".into(),
        contract: TypeContract::SingleOperation {
            requirement: SingleOperation {
                input: "start".into(),
                outputs: vec!["arrived".into()],
                initial: vec![false],
                completed: vec![true],
            },
        },
    }
}

#[test]
fn single_operation_archives_require_v12_and_reject_invalid_observations() {
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_type(definition()).unwrap();
    let archive = catalog.to_json().unwrap();
    assert!(archive.contains("dustroute.blueprint-catalog.v12"));
    assert_eq!(BlueprintCatalog::from_json(&archive).unwrap(), catalog);
    for version in 1..=11 {
        assert!(
            BlueprintCatalog::from_json(
                &archive.replace("catalog.v12", &format!("catalog.v{version}"))
            )
            .is_err()
        );
    }
    for bad in 0..3 {
        let mut invalid = definition();
        let TypeContract::SingleOperation { requirement } = &mut invalid.contract else {
            unreachable!()
        };
        match bad {
            0 => requirement.completed.clear(),
            1 => requirement.outputs[0] = requirement.input.clone(),
            _ => requirement.input.clear(),
        }
        assert!(BlueprintCatalog::default().insert_type(invalid).is_err());
    }
}
