use super::*;

#[must_use]
pub fn propose_scenarios(
    snapshot: &MinecraftSnapshot,
    analysis: &PhysicalAnalysis,
) -> Vec<Scenario> {
    // An Observer is not a steady-state source: a lever transition is visible
    // as a one-redstone-tick pulse at its back. Choose the pulse capability for
    // generated scenarios so MCP callers do not receive a scenario that is
    // rejected merely because an Observer is present.
    let required_capability = world_from_snapshot(snapshot)
        .ok()
        .filter(|world| {
            world
                .iter()
                .any(|(_, block)| block.kind == BlockKind::Observer)
        })
        .map_or(ScenarioCapability::SteadyPower, |_| {
            ScenarioCapability::ObserverPulse
        });
    analysis
        .reverse
        .analysis
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            Some(Scenario {
                label: format!("toggle inferred input {index}"),
                initial: snapshot.clone(),
                actions: inferred_input_actions(snapshot, &analysis.reverse.analysis, input)?,
                observe: analysis
                    .reverse
                    .analysis
                    .outputs
                    .iter()
                    .map(|output| output.anchor)
                    .collect(),
                duration_redstone_ticks: 10,
                required_capabilities: vec![required_capability],
                expectation: ScenarioExpectation::default(),
            })
        })
        .collect()
}

fn inferred_input_actions(
    snapshot: &MinecraftSnapshot,
    analysis: &crate::world_reverse::RegionAnalysis,
    input: &crate::world_reverse::InferredTerminal,
) -> Option<Vec<ScenarioAction>> {
    let Ok(world) = world_from_snapshot(snapshot) else {
        return None;
    };
    let Ok(driver) = inferred_input_driver(&world, analysis, input) else {
        return None;
    };
    Some(match driver {
        crate::world_reverse::InferredInputDriver::Lever(position) => vec![
            ScenarioAction::SetLeverState {
                redstone_tick: 1,
                position,
                powered: true,
            },
            ScenarioAction::SetLeverState {
                redstone_tick: 5,
                position,
                powered: false,
            },
        ],
        crate::world_reverse::InferredInputDriver::Button(position) => vec![
            ScenarioAction::PressButton {
                redstone_tick: 1,
                position,
            },
            ScenarioAction::ReleaseButton {
                redstone_tick: 5,
                position,
            },
        ],
        crate::world_reverse::InferredInputDriver::PressurePlate(position) => vec![
            ScenarioAction::SetPressurePlateLevel {
                redstone_tick: 1,
                position,
                level: 15,
            },
            ScenarioAction::SetPressurePlateLevel {
                redstone_tick: 5,
                position,
                level: 0,
            },
        ],
        crate::world_reverse::InferredInputDriver::External(position) => vec![
            ScenarioAction::SetExternalPower {
                redstone_tick: 1,
                position,
                powered: true,
            },
            ScenarioAction::SetExternalPower {
                redstone_tick: 5,
                position,
                powered: false,
            },
        ],
    })
}

pub fn simulate_scenario(scenario: &Scenario) -> Result<ScenarioRun, String> {
    run_scenario(scenario)
}

#[must_use]
pub fn compare_live_trace(
    expected: &ScenarioTrace,
    actual: &ScenarioTrace,
) -> Vec<ScenarioDifference> {
    compare_scenario_traces(expected, actual)
}
