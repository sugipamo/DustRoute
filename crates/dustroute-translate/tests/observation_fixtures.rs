use dustroute_ir::derive_hierarchy;
use dustroute_physical::{BlockKind, CapabilityLevel, CapabilityStage, Pos};
use dustroute_translate::{
    DeviceOutputState, RegionBounds, ReverseRequest, Translator, analyze_world_region,
    extract_connectivity, solve_instantaneous, world_from_snapshot_json,
};

fn load(
    source: &str,
) -> (
    dustroute_translate::MinecraftSnapshot,
    dustroute_translate::World,
) {
    world_from_snapshot_json(source).expect("fixture must be a valid lossless observation")
}

fn analyze(source: &str) -> dustroute_translate::RegionAnalysis {
    let (snapshot, world) = load(source);
    analyze_world_region(&world, RegionBounds::new(snapshot.min, snapshot.max))
}

#[test]
fn all_observation_regression_fixtures_import() {
    for fixture in [
        include_str!("fixtures/normal_wire.json"),
        include_str!("fixtures/broken_wire.json"),
        include_str!("fixtures/reversed_repeater.json"),
        include_str!("fixtures/missing_support.json"),
        include_str!("fixtures/torch_not.json"),
        include_str!("fixtures/wire_merge.json"),
        include_str!("fixtures/delayed_path.json"),
        include_str!("fixtures/unsupported_comparator.json"),
        include_str!("fixtures/boundary_limited.json"),
        include_str!("fixtures/vertical_support_matrix.json"),
        include_str!("fixtures/comparator_modes_live.json"),
    ] {
        let (snapshot, world) = load(fixture);
        assert!(!snapshot.blocks.is_empty());
        assert!(world.iter().all(|(_, block)| block.observed_name.is_some()));
    }
}

#[test]
fn live_comparator_modes_match_analog_simulation() {
    let (_, world) = load(include_str!("fixtures/comparator_modes_live.json"));
    let mut simulator = dustroute_translate::RedstoneTickSimulator::new(world).unwrap();
    let state = simulator.advance_tick().unwrap();
    assert_eq!(state.strength(Pos::new(2, 1, 0)), 15);
    assert_eq!(state.strength(Pos::new(2, 1, 5)), 1);
}

#[test]
fn live_vertical_support_matrix_matches_direction_shape_and_strength() {
    let (_, world) = load(include_str!("fixtures/vertical_support_matrix.json"));
    let graph = extract_connectivity(&world);
    let state = solve_instantaneous(&world, &DeviceOutputState::default(), 256).unwrap();

    assert!(graph.can_reach(Pos::new(0, 1, 0), Pos::new(1, 2, 0)));
    assert!(!graph.can_reach(Pos::new(1, 2, 5), Pos::new(0, 1, 5)));
    assert!(graph.can_reach(Pos::new(10, 1, 0), Pos::new(11, 2, 0)));
    assert!(!graph.can_reach(Pos::new(11, 2, 5), Pos::new(10, 1, 5)));
    assert!(graph.can_reach(Pos::new(21, 2, 5), Pos::new(20, 1, 5)));
    assert!(graph.can_reach(Pos::new(25, 1, 0), Pos::new(26, 2, 0)));
    assert!(!graph.can_reach(Pos::new(26, 2, 5), Pos::new(25, 1, 5)));
    assert!(!graph.can_reach(Pos::new(30, 1, 0), Pos::new(31, 2, 0)));

    for (pos, block) in world
        .iter()
        .filter(|(_, block)| block.power_level.is_some())
    {
        assert_eq!(state.signal(*pos), block.power_level.unwrap(), "at {pos:?}");
    }
}

#[test]
fn broken_wire_and_missing_support_remain_physical_evidence() {
    let broken = analyze(include_str!("fixtures/broken_wire.json"));
    assert!(!broken.scene.gap_candidates(2).is_empty());

    let unsupported = analyze(include_str!("fixtures/missing_support.json"));
    assert!(!unsupported.diagnostics.invalid_supports.is_empty());
}

#[test]
fn direction_merge_and_delay_survive_reverse_translation() {
    let reversed = analyze(include_str!("fixtures/reversed_repeater.json"));
    let repeater = reversed
        .scene
        .components
        .iter()
        .find(|component| component.block.kind == BlockKind::Repeater)
        .unwrap();
    assert_eq!(repeater.block.observed_properties["facing"], "east");

    let merged = analyze(include_str!("fixtures/wire_merge.json"));
    assert!(merged.scene.connections.len() >= 3);

    let (snapshot, world) = load(include_str!("fixtures/delayed_path.json"));
    let translated = Translator.reverse(
        &world,
        ReverseRequest::new(RegionBounds::new(snapshot.min, snapshot.max)),
    );
    assert!(translated.temporal.behavior.devices.iter().any(|device| {
        device.minimum_delay_redstone_ticks == 4
            && translated
                .analysis
                .scene
                .component_at(device.physical_position)
                .is_some()
    }));
}

#[test]
fn newly_supported_comparator_and_scan_boundaries_are_explicit() {
    let comparator = analyze(include_str!("fixtures/unsupported_comparator.json"));
    let report = comparator.scene.capability_report();
    assert!(!report.issues.iter().any(|issue| {
        issue.kind == BlockKind::Comparator
            && issue.stage == CapabilityStage::SteadyState
            && issue.level == CapabilityLevel::Unsupported
    }));
    let hierarchy = derive_hierarchy(&comparator.scene);
    assert!(
        !hierarchy
            .cell_graph
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "steady_state_semantics_unsupported" })
    );
    assert!(report.issues.iter().any(|issue| {
        issue.kind == BlockKind::Comparator
            && issue.stage == CapabilityStage::Temporal
            && issue.level == CapabilityLevel::Partial
    }));

    let boundary = analyze(include_str!("fixtures/boundary_limited.json"));
    assert!(!boundary.scene.observation.frontier.is_empty());
    assert!(
        boundary
            .scene
            .open_frontier_components()
            .iter()
            .any(|component| {
                boundary
                    .scene
                    .components
                    .iter()
                    .any(|item| item.id == *component && item.pos == Pos::new(1, 1, 0))
            })
    );
}
