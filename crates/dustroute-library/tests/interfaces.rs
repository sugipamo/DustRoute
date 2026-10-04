#[path = "support/catalog_fixture.rs"]
mod catalog_fixture;
use catalog_fixture::FixtureJson;
use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_minecraft::{Pos, RotationY};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn port(path: &[&str], name: &str) -> BlueprintPortRef {
    BlueprintPortRef {
        instance: path
            .iter()
            .map(|name| InstanceId::new(*name).unwrap())
            .collect(),
        port: name.into(),
    }
}
fn wrapper(
    catalog: &BlueprintCatalog,
    child: &BlueprintRevisionId,
    name: &str,
    rotation: RotationY,
) -> BlueprintRevision {
    let mut parent = catalog.revision(child).unwrap().clone();
    parent.id = id(name);
    parent.parents.clear();
    parent.blocks.clear();
    parent.connections.clear();
    parent.inclusions = vec![BlueprintInclusion {
        instance: InstanceId::new("child").unwrap(),
        revision: child.clone(),
        origin: Pos::default(),
        rotation,
    }];
    parent.port_bindings = parent
        .ports
        .iter()
        .map(|p| BlueprintPortBinding {
            name: p.name.clone(),
            port: port(&["child"], &p.name),
        })
        .collect();
    for p in &mut parent.ports {
        p.position = rotation.pos(p.position);
        p.facing = p.facing.map(|facing| rotation.facing(facing));
    }
    parent
}
fn proposal(catalog: &BlueprintCatalog, source: &BlueprintRevisionId) -> Assembly {
    Assembly {
        name: "Reused arrangement".into(),
        instances: vec![BlueprintInclusion {
            instance: InstanceId::new("root").unwrap(),
            revision: source.clone(),
            origin: Pos::default(),
            rotation: RotationY::R0,
        }],
        blocks: catalog
            .expand(source)
            .unwrap()
            .blocks
            .into_iter()
            .map(|(position, block)| PositionedBlock { position, block })
            .collect(),
        known_regions: vec![],
        connections: vec![],
        boundaries: vec![],
    }
}

#[test]
fn nested_aliases_keep_the_leaf_frame_and_all_consumer_requirements() {
    let mut catalog = builtin_blueprints().clone();
    let mut leaf = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    leaf.id = id("typed-not.v1");
    leaf.ports[0].required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(leaf.clone()).unwrap();
    let mut first = wrapper(&catalog, &leaf.id, "first.v1", RotationY::R90);
    first.ports[0].required_source_types =
        vec![TypeRevisionId::new(BLOCK_POWER_TYPE_REVISION).unwrap()];
    catalog.insert_revision(first.clone()).unwrap();
    let mut outer = wrapper(&catalog, &first.id, "outer.v1", RotationY::R90);
    outer.ports[0].required_source_types.clear();
    catalog.insert_revision(outer.clone()).unwrap();
    let loaded = catalog_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    let (resolved, rotation) = loaded.resolve_port(&outer.id, &port(&[], "a")).unwrap();
    assert_eq!(rotation, RotationY::R180);
    assert_eq!(
        resolved.position,
        RotationY::R180.pos(leaf.ports[0].position)
    );
    assert_eq!(resolved.required_source_types.len(), 2);
    let assembly = proposal(&loaded, &outer.id);
    let view = assembly.inspect(&loaded).unwrap();
    assert_eq!(
        view.canonical_port_ref(&port(&["root"], "a")).unwrap(),
        &port(&["root", "child", "child"], "a")
    );
    assert_eq!(
        view.resolved_port(&port(&["root", "child", "child"], "a"))
            .unwrap()
            .0
            .required_source_types
            .len(),
        2
    );
    assert_eq!(catalog.revision(&leaf.id), Some(&leaf));
}

#[test]
fn bindings_cannot_hide_a_different_or_missing_physical_terminal() {
    let mut catalog = builtin_blueprints().clone();
    let parent = wrapper(&catalog, &id(NOT_TOP_REVISION), "invalid.v1", RotationY::R0);
    for variant in 0..6 {
        let mut invalid = parent.clone();
        match variant {
            0 => invalid.port_bindings[0].port.instance.clear(),
            1 => invalid.port_bindings[0].port.port = "missing".into(),
            2 => invalid.ports[0].position.x += 1,
            3 => invalid.ports[0].kind = BlueprintPortKind::BlockState,
            4 => invalid.ports[0].facing = None,
            _ => invalid.port_bindings.push(invalid.port_bindings[0].clone()),
        }
        assert!(
            catalog.insert_revision(invalid).is_err(),
            "variant {variant}"
        );
        assert!(catalog.revision(&parent.id).is_none());
    }
}

#[test]
fn conflicting_inherited_routes_need_explicit_actual_state_without_rebinding_sources() {
    let mut catalog = builtin_blueprints().clone();
    let mut parent = wrapper(&catalog, &id(NOT_TOP_REVISION), "pair.v1", RotationY::R0);
    parent.inclusions.push(BlueprintInclusion {
        instance: InstanceId::new("second").unwrap(),
        revision: id(NOT_TOP_REVISION),
        origin: Pos::new(8, 0, 0),
        rotation: RotationY::R0,
    });
    parent.connections.push(BlueprintConnection {
        source: port(&["child"], "out"),
        sink: port(&["second"], "a"),
        path: vec![
            parent
                .ports
                .iter()
                .find(|p| p.name == "out")
                .unwrap()
                .position,
            parent
                .ports
                .iter()
                .find(|p| p.name == "a")
                .unwrap()
                .position
                .offset(8, 0, 0),
        ],
    });
    catalog.insert_revision(parent.clone()).unwrap();
    let mut outer = wrapper(&catalog, &parent.id, "rerouted-pair.v1", RotationY::R0);
    outer.connections = parent.connections.clone();
    let edge = &mut outer.connections[0];
    edge.source
        .instance
        .insert(0, InstanceId::new("child").unwrap());
    edge.sink
        .instance
        .insert(0, InstanceId::new("child").unwrap());
    edge.path.insert(1, Pos::new(5, 1, 1));
    catalog.insert_revision(outer.clone()).unwrap();
    let mut actual = proposal(&catalog, &outer.id);
    assert!(actual.with_source_connections(&catalog).is_err());
    let mut chosen = outer.connections[0].clone();
    chosen
        .source
        .instance
        .insert(0, InstanceId::new("root").unwrap());
    chosen
        .sink
        .instance
        .insert(0, InstanceId::new("root").unwrap());
    actual.connections.push(chosen);
    let retained = actual.with_source_connections(&catalog).unwrap();
    assert_eq!(retained, actual);
    assert_eq!(catalog.revision(&parent.id), Some(&parent));
    assert_eq!(catalog.revision(&outer.id), Some(&outer));
    // This was structural resolution only. Jumping routes have not acquired
    // physical connectivity evidence merely by being saved or selected.
    let mut inherited = proposal(&catalog, &parent.id)
        .with_source_connections(&catalog)
        .unwrap();
    assert_eq!(inherited.connections.len(), 1);
    inherited.connections.clear();
    assert!(inherited.inspect(&catalog).is_ok());
}
