use super::*;

#[must_use]
pub fn explain_signal_path(analysis: &PhysicalAnalysis, from: Pos, to: Pos) -> SignalPath {
    let scene = &analysis.reverse.analysis.scene;
    let Some(source) = scene.component_at(from).map(|component| component.id) else {
        return missing_path(from, to, "source is not a modeled circuit component");
    };
    let Some(sink) = scene.component_at(to).map(|component| component.id) else {
        return missing_path(from, to, "destination is not a modeled circuit component");
    };
    let mut queue = VecDeque::from([source]);
    let mut previous = BTreeMap::new();
    let mut visited = BTreeSet::from([source]);
    while let Some(current) = queue.pop_front() {
        if current == sink {
            break;
        }
        for connection in scene
            .connections
            .iter()
            .filter(|edge| edge.source.component == current)
        {
            let next = connection.sink.component;
            if visited.insert(next) {
                previous.insert(next, (current, connection.transfer));
                queue.push_back(next);
            }
        }
    }
    if !visited.contains(&sink) {
        return missing_path(from, to, "no directed signal path was found");
    }
    let mut components = vec![sink];
    let mut transfers = Vec::new();
    let mut current = sink;
    while current != source {
        let (parent, transfer) = previous[&current];
        components.push(parent);
        transfers.push(transfer);
        current = parent;
    }
    components.reverse();
    transfers.reverse();
    let positions = components
        .iter()
        .filter_map(|id| scene.components.iter().find(|c| c.id == *id).map(|c| c.pos))
        .collect::<Vec<_>>();
    SignalPath {
        complete: scene.observation.is_complete(),
        explanation: format!(
            "directed path through {} physical components",
            positions.len()
        ),
        positions,
        transfers,
    }
}

/// Builds a small explanation around `target` while retaining the full
/// directed path evidence needed by an LLM to reason about local roles.  Path
/// enumeration is deliberately capped; callers should request a larger
/// region explicitly rather than making a gaze query unbounded.
#[must_use]
pub fn explain_focused_component(
    analysis: &PhysicalAnalysis,
    target: Pos,
    analysis_complete: bool,
) -> FocusedExplanation {
    const MAX_TERMINALS: usize = 16;
    const MAX_PATHS: usize = 8;
    let scene = &analysis.reverse.analysis.scene;
    let role = classify_focused_role(&analysis.reverse, target);
    let component_positions = scene
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect::<BTreeMap<_, _>>();
    let physical_component = scene.component_at(target).map(|component| component.id);
    let (block, observed_name, observed_properties) = scene
        .component_at(target)
        .map(|component| {
            (
                Some(component.block.kind),
                component.block.observed_name.clone(),
                component.block.observed_properties.clone(),
            )
        })
        .unwrap_or((None, None, BTreeMap::new()));
    let adjacent = |incoming: bool| {
        scene
            .connections
            .iter()
            .filter(|connection| {
                physical_component.is_some_and(|component| {
                    if incoming {
                        connection.sink.component == component
                    } else {
                        connection.source.component == component
                    }
                })
            })
            .filter_map(|connection| {
                let source_position = component_positions.get(&connection.source.component)?;
                let sink_position = component_positions.get(&connection.sink.component)?;
                Some(FocusedConnection {
                    source_component: connection.source.component,
                    source_position: *source_position,
                    sink_component: connection.sink.component,
                    sink_position: *sink_position,
                    transfer: connection.transfer,
                    confidence: connection.confidence,
                })
            })
            .collect::<Vec<_>>()
    };
    let input_candidates = analysis
        .reverse
        .analysis
        .inputs
        .iter()
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let output_candidates = analysis
        .reverse
        .analysis
        .outputs
        .iter()
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let paths_from_inputs = input_candidates
        .iter()
        .take(MAX_PATHS)
        .map(|input| FocusedPath {
            endpoint: input.anchor,
            direction: "input_to_focus".to_owned(),
            path: explain_signal_path(analysis, input.anchor, target),
        })
        .collect::<Vec<_>>();
    let paths_to_outputs = output_candidates
        .iter()
        .take(MAX_PATHS)
        .map(|output| FocusedPath {
            endpoint: output.anchor,
            direction: "focus_to_output".to_owned(),
            path: explain_signal_path(analysis, target, output.anchor),
        })
        .collect::<Vec<_>>();
    let timing = scene.temporal_assessment();
    let temporal_devices = analysis
        .hierarchy
        .temporal
        .behavior
        .devices
        .iter()
        .filter(|device| {
            physical_component.is_some_and(|component| {
                device.component == component
                    || role.incoming_components.contains(&device.component.0)
                    || role.outgoing_components.contains(&device.component.0)
            })
        })
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let observation_complete = analysis_complete && scene.observation.is_complete();
    let mut caveats = Vec::new();
    if !observation_complete {
        caveats.push(
            "the focused explanation is bounded by an incomplete or open observation".to_owned(),
        );
    }
    if timing.requirement != dustroute_physical::TemporalRequirement::SteadyStateSafe {
        caveats.push(format!(
            "timing requires {:?}; same-tick order is evidence, not a vanilla scheduler proof",
            timing.requirement
        ));
    }
    if input_candidates.is_empty() {
        caveats.push("no mapped input terminal was found in the observed region".to_owned());
    }
    if output_candidates.is_empty() {
        caveats.push(
            "no mapped observable output terminal was found in the observed region".to_owned(),
        );
    }
    FocusedExplanation {
        position: target,
        block,
        observed_name,
        observed_properties,
        physical_component,
        role,
        incoming: adjacent(true),
        outgoing: adjacent(false),
        input_candidates,
        output_candidates,
        paths_from_inputs,
        paths_to_outputs,
        timing,
        temporal_devices,
        observation_complete,
        caveats,
    }
}

/// Produces the same physical-first shape for the hierarchical fast path.
/// Flat terminal inference is intentionally omitted for large regions, so the
/// returned candidate/path arrays remain empty and the caveat says why.
#[must_use]
pub fn explain_focused_scene(
    scene: &dustroute_physical::PhysicalScene,
    hierarchy: &dustroute_ir::HierarchicalIr,
    target: Pos,
    analysis_complete: bool,
) -> FocusedExplanation {
    let physical_component = scene.component_at(target).map(|component| component.id);
    let component_positions = scene
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect::<BTreeMap<_, _>>();
    let (block, observed_name, observed_properties) = scene
        .component_at(target)
        .map(|component| {
            (
                Some(component.block.kind),
                component.block.observed_name.clone(),
                component.block.observed_properties.clone(),
            )
        })
        .unwrap_or((None, None, BTreeMap::new()));
    let (incoming_components, outgoing_components) = physical_component
        .map(|component| {
            let incoming = scene
                .connections
                .iter()
                .filter(|edge| edge.sink.component == component)
                .map(|edge| edge.source.component.0)
                .collect::<BTreeSet<_>>();
            let outgoing = scene
                .connections
                .iter()
                .filter(|edge| edge.source.component == component)
                .map(|edge| edge.sink.component.0)
                .collect::<BTreeSet<_>>();
            (incoming, outgoing)
        })
        .unwrap_or_default();
    let has_feedback = physical_component.is_some_and(|component| {
        scene
            .connections
            .iter()
            .any(|edge| edge.source.component == component && edge.sink.component == component)
    });
    let role_kind = if physical_component.is_none() {
        LocalSignalRole::SupportOrUnresolved
    } else if incoming_components.len() > 1 {
        LocalSignalRole::SignalMerge
    } else if outgoing_components.len() > 1 {
        LocalSignalRole::SignalBranch
    } else if has_feedback {
        LocalSignalRole::FeedbackPath
    } else if !incoming_components.is_empty() || !outgoing_components.is_empty() {
        LocalSignalRole::IntermediatePath
    } else {
        LocalSignalRole::SupportOrUnresolved
    };
    let role = FocusedRole {
        position: target,
        physical_component,
        signal_component: physical_component.map(|component| component.0),
        incoming_components,
        outgoing_components,
        role: role_kind,
    };
    let connections = |incoming: bool| {
        scene
            .connections
            .iter()
            .filter(|edge| {
                physical_component.is_some_and(|component| {
                    if incoming {
                        edge.sink.component == component
                    } else {
                        edge.source.component == component
                    }
                })
            })
            .filter_map(|edge| {
                Some(FocusedConnection {
                    source_component: edge.source.component,
                    source_position: *component_positions.get(&edge.source.component)?,
                    sink_component: edge.sink.component,
                    sink_position: *component_positions.get(&edge.sink.component)?,
                    transfer: edge.transfer,
                    confidence: edge.confidence,
                })
            })
            .collect::<Vec<_>>()
    };
    let timing = scene.temporal_assessment();
    let temporal_devices = hierarchy
        .temporal
        .behavior
        .devices
        .iter()
        .filter(|device| {
            physical_component.is_some_and(|component| {
                device.component == component
                    || role.incoming_components.contains(&device.component.0)
                    || role.outgoing_components.contains(&device.component.0)
            })
        })
        .take(16)
        .cloned()
        .collect::<Vec<_>>();
    let observation_complete = analysis_complete && scene.observation.is_complete();
    let mut caveats = vec![
        "flat terminal and path inference was skipped for this hierarchical region".to_owned(),
    ];
    if !observation_complete {
        caveats.push(
            "the focused explanation is bounded by an incomplete or open observation".to_owned(),
        );
    }
    if timing.requirement != dustroute_physical::TemporalRequirement::SteadyStateSafe {
        caveats.push(format!(
            "timing requires {:?}; same-tick order is evidence, not a vanilla scheduler proof",
            timing.requirement
        ));
    }
    FocusedExplanation {
        position: target,
        block,
        observed_name,
        observed_properties,
        physical_component,
        role,
        incoming: connections(true),
        outgoing: connections(false),
        input_candidates: Vec::new(),
        output_candidates: Vec::new(),
        paths_from_inputs: Vec::new(),
        paths_to_outputs: Vec::new(),
        timing,
        temporal_devices,
        observation_complete,
        caveats,
    }
}

fn missing_path(from: Pos, to: Pos, reason: &str) -> SignalPath {
    SignalPath {
        positions: vec![from, to],
        transfers: Vec::new(),
        complete: false,
        explanation: reason.to_owned(),
    }
}
