//! Bounded, type-directed optimization with an explicit physical cost domain.
//!
//! Produces new immutable candidate data. No source is rewritten, no parent is
//! updated, and neither a report nor a minimum in this search is global proof.
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use dustroute_library::PortDirection;
use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::behavior_type::{PhysicalBehaviorContext, PhysicalInputDriver};
use dustroute_library::blueprint::*;
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::blueprint_update::RecordedReview;
use dustroute_translate::physical_behavior::{
    PhysicalBehaviorModel, PhysicalBehaviorSelection, PhysicalOutput,
};
use dustroute_translate::promotion::{CheckStatus, PromotionReport, review_assembly_in_context};
use dustroute_translate::{Block, BlockKind, Pos};
use serde::{Deserialize, Serialize};

mod enumeration;
mod scope;
pub use enumeration::{
    EnumeratedTorchSupport, TorchSupportEnumerationReport, TorchSupportEnumerationRequest,
    enumerate_torch_supports,
};
pub use scope::BlueprintReductionScope;
use scope::SearchScope;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlueprintReductionRequest {
    /// Complete actual state used for physics, including external equipment.
    /// The scope separately selects which occupied blocks are optimized/counted.
    pub base_state: AssemblyRevisionId,
    #[serde(default)]
    pub scope: BlueprintReductionScope,
    /// Selects the interpretation whose named ports bind the target type.
    pub target_instance: InstancePath,
    pub target: BehaviorBinding,
    pub candidate_revision: BlueprintRevisionId,
    pub candidate_state: AssemblyRevisionId,
    pub behavior_context: PhysicalBehaviorContext,
    /// Explicit arbitrary rewrites may change blocks, ports, drivers and nesting.
    /// All retained interpretations are rechecked. No classifications imply proof.
    #[serde(default)]
    pub alternatives: Vec<BlueprintReductionCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlueprintReductionCandidate {
    pub blueprint: BlueprintRevision,
    pub state: AssemblyRevision,
    pub behavior_context: PhysicalBehaviorContext,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlueprintReductionBudget {
    pub max_layouts: usize,
    pub max_bindings: usize,
    /// Total search time, including baseline and every fresh verification.
    pub max_millis: u64,
}
impl Default for BlueprintReductionBudget {
    fn default() -> Self {
        Self {
            max_layouts: 4096,
            max_bindings: 256,
            max_millis: 30_000,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BlueprintReductionStats {
    pub layouts_examined: usize,
    pub bindings_examined: usize,
    pub passed: usize,
    pub failed: usize,
    pub undetermined: usize,
    /// Rejected only after a complete concrete held-input cycle contains an
    /// incorrect output. A finite transient or unfinished screen cannot reject.
    pub screened_counterexamples: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReviewedReductionCandidate {
    pub candidate: BlueprintReductionCandidate,
    pub occupied_blocks: usize,
    /// Complete physical Assembly, including external equipment.
    pub total_occupied_blocks: usize,
    pub review: RecordedReview,
}

#[derive(Clone, Debug, Serialize)]
pub struct BlueprintReductionReport {
    pub base_state: AssemblyRevisionId,
    pub target_type: TypeRevisionId,
    pub baseline_blocks: usize,
    pub baseline_total_blocks: usize,
    pub cost_scope: BlueprintReductionScope,
    /// This checks the selected type after explicitly projecting away other
    /// interpretations; it does not certify the original Assembly's children.
    pub baseline_scope: &'static str,
    pub baseline: RecordedReview,
    /// Strictly smaller and freshly passed in the selected model, or none.
    pub best: Option<ReviewedReductionCandidate>,
    /// Flattening removes these old interpretation claims from the generated
    /// candidate, not from the old Assembly or its immutable source records.
    pub prior_interpretations: Vec<BlueprintOccurrence>,
    pub stats: BlueprintReductionStats,
    pub stop_reason: String,
    /// Always false: finite subset enumeration and supplied alternatives cannot
    /// prove a global minimum over all possible realizations of the type.
    pub global_minimality_proven: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Terminal {
    position: Pos,
    block_power: bool,
    device_output: bool,
}
impl Terminal {
    fn from_port(port: &BlueprintPort) -> Result<Self, String> {
        if port.kind == BlueprintPortKind::BlockState {
            return Err("behavior requires signal terminals".into());
        }
        Ok(Self {
            position: port.position,
            block_power: port.kind == BlueprintPortKind::BlockPower,
            device_output: port.kind == BlueprintPortKind::DeviceOutput,
        })
    }
    fn kind(self) -> BlueprintPortKind {
        if self.device_output {
            BlueprintPortKind::DeviceOutput
        } else if self.block_power {
            BlueprintPortKind::BlockPower
        } else {
            BlueprintPortKind::Wire
        }
    }
    fn port(self, name: &str, direction: PortDirection) -> BlueprintPort {
        BlueprintPort {
            name: name.into(),
            direction,
            position: self.position,
            kind: self.kind(),
            facing: None,
            required_source_types: vec![],
        }
    }
}

struct Target {
    inputs: Vec<String>,
    outputs: Vec<String>,
    controls: Vec<Pos>,
    binding: BehaviorBinding,
    scope: SearchScope,
}

fn fresh_budget(started: Instant, budget: BlueprintReductionBudget) -> BehaviorBudget {
    let defaults = BehaviorBudget::default();
    BehaviorBudget {
        max_elapsed: defaults
            .max_elapsed
            .min(Duration::from_millis(budget.max_millis).saturating_sub(started.elapsed())),
        ..defaults
    }
}
fn time_left(started: Instant, budget: BlueprintReductionBudget) -> bool {
    started.elapsed() < Duration::from_millis(budget.max_millis)
}
fn evaluate(
    catalog: &BlueprintCatalog,
    candidate: &BlueprintReductionCandidate,
    budget: BehaviorBudget,
) -> Result<PromotionReport, String> {
    let mut staged = catalog.clone();
    staged
        .insert_revision(candidate.blueprint.clone())
        .map_err(|e| e.to_string())?;
    staged
        .insert_assembly(candidate.state.clone())
        .map_err(|e| e.to_string())?;
    review_assembly_in_context(
        &staged,
        &candidate.state.assembly,
        Some(&candidate.behavior_context),
        budget,
    )
    .map_err(|e| e.to_string())
}

fn target_ports(
    catalog: &BlueprintCatalog,
    request: &BlueprintReductionRequest,
) -> Result<(Target, Vec<BlueprintPort>), String> {
    let base = catalog
        .assembly(&request.base_state)
        .ok_or("unknown base Assembly")?;
    let view = base.assembly.inspect(catalog).map_err(|e| e.to_string())?;
    let definition = catalog
        .type_revision(request.target.behavior_type())
        .ok_or("unknown target type")?;
    let (inputs, outputs, mapping_in, mapping_out) = match (&definition.contract, &request.target) {
        (
            TypeContract::RepeatedSettling { relation },
            BehaviorBinding::RepeatedSettling {
                inputs, outputs, ..
            },
        ) => (
            relation.inputs.clone(),
            relation.outputs.clone(),
            inputs.clone(),
            outputs.clone(),
        ),
        (
            TypeContract::Periodic {
                requirement: dustroute_library::behavior_type::Periodic { output },
            }
            | TypeContract::FiniteBurst {
                requirement: dustroute_library::behavior_type::FiniteBurst { output },
            },
            BehaviorBinding::Autonomous { output_port, .. },
        ) => (
            vec![],
            vec![output.clone()],
            BTreeMap::new(),
            BTreeMap::from([(output.clone(), output_port.clone())]),
        ),
        _ => return Err("target binding must match a supported behavioral type".into()),
    };
    if mapping_in.keys().collect::<BTreeSet<_>>() != inputs.iter().collect::<BTreeSet<_>>()
        || mapping_out.keys().collect::<BTreeSet<_>>() != outputs.iter().collect::<BTreeSet<_>>()
    {
        return Err("target mapping must cover all named type ports".into());
    }
    let mut ports = vec![];
    let mut controls = vec![];
    for (names, mapping, direction) in [
        (&inputs, &mapping_in, PortDirection::Input),
        (&outputs, &mapping_out, PortDirection::Output),
    ] {
        for name in names {
            let (port, _) = view
                .resolved_port(&BlueprintPortRef {
                    instance: request.target_instance.clone(),
                    port: mapping[name].clone(),
                })
                .map_err(|e| e.to_string())?;
            if port.direction != direction {
                return Err("target port has the wrong direction".into());
            }
            let terminal = Terminal::from_port(&port)?;
            if direction == PortDirection::Input {
                let drivers: Vec<_> = request
                    .behavior_context
                    .input_drivers
                    .iter()
                    .filter(|d| d.port_position == port.position && d.port_kind == port.kind)
                    .collect();
                if drivers.len() != 1 {
                    return Err("each selected input needs exactly one actual driver".into());
                }
                controls.push(drivers[0].lever_position);
            }
            ports.push(terminal.port(name, direction));
        }
    }
    let binding = if inputs.is_empty() {
        BehaviorBinding::Autonomous {
            behavior_type: definition.id.clone(),
            output_port: outputs[0].clone(),
        }
    } else {
        BehaviorBinding::RepeatedSettling {
            behavior_type: definition.id.clone(),
            inputs: inputs.iter().map(|n| (n.clone(), n.clone())).collect(),
            outputs: outputs.iter().map(|n| (n.clone(), n.clone())).collect(),
        }
    };
    Ok((
        Target {
            inputs,
            outputs,
            controls,
            binding,
            scope: SearchScope::resolve(catalog, request, base)?,
        },
        ports,
    ))
}

fn context_with_ports(
    context: &PhysicalBehaviorContext,
    target: &Target,
    ports: &[BlueprintPort],
) -> PhysicalBehaviorContext {
    let mut context = context.clone();
    context.input_drivers = ports
        .iter()
        .take(target.inputs.len())
        .zip(&target.controls)
        .map(|(port, lever)| PhysicalInputDriver {
            port_position: port.position,
            port_kind: port.kind,
            lever_position: *lever,
        })
        .collect();
    context
}

fn make_candidate(
    catalog: &BlueprintCatalog,
    request: &BlueprintReductionRequest,
    target: &Target,
    ports: Vec<BlueprintPort>,
    blocks: Vec<PositionedBlock>,
) -> Result<BlueprintReductionCandidate, String> {
    let base = catalog
        .assembly(&request.base_state)
        .ok_or("unknown base Assembly")?;
    let view = base.assembly.inspect(catalog).map_err(|e| e.to_string())?;
    let source = view
        .occurrences
        .get(&request.target_instance)
        .ok_or("unknown target occurrence")?;
    let source = catalog.revision(&source.revision).expect("indexed source");
    let root = InstanceId::new("optimized").expect("valid internal root name");
    let context = context_with_ports(&request.behavior_context, target, &ports);
    let blueprint = BlueprintRevision {
        id: request.candidate_revision.clone(),
        parents: vec![source.id.clone()],
        name: format!("Type-preserving candidate for {}", source.name),
        classifications: vec![],
        required_laws: context.law_revisions().into_iter().cloned().collect(),
        static_type_bindings: vec![],
        behavior_bindings: vec![target.binding.clone()],
        blocks: vec![],
        initial_layout: Some(BlueprintLayout {
            blocks: blocks
                .iter()
                .filter(|b| target.scope.owns(b.position))
                .cloned()
                .collect(),
            known_regions: if target.scope.component {
                vec![]
            } else {
                base.assembly.known_regions.clone()
            },
        }),
        law: None,
        inclusions: vec![],
        ports: ports.clone(),
        connections: vec![],
        port_bindings: vec![],
        provenance: dustroute_library::Provenance {
            author: "DustRoute type-directed optimizer".into(),
            source_url: None,
            license: source.provenance.license.clone(),
            retrieved_on: None,
        },
    };
    let mut boundary_names: BTreeSet<_> = target
        .scope
        .environment_boundaries
        .iter()
        .map(|boundary| boundary.name.clone())
        .collect();
    let state = AssemblyRevision {
        id: request.candidate_state.clone(),
        parents: vec![base.id.clone()],
        assembly: Assembly {
            name: blueprint.name.clone(),
            instances: std::iter::once(BlueprintInclusion {
                instance: root.clone(),
                revision: blueprint.id.clone(),
                origin: Pos::default(),
                rotation: Default::default(),
            })
            .chain(target.scope.environment_instances.clone())
            .collect(),
            blocks,
            known_regions: base.assembly.known_regions.clone(),
            connections: target.scope.environment_connections.clone(),
            boundaries: ports
                .into_iter()
                .map(|port| BlueprintPortBinding {
                    name: {
                        let mut name = port.name.clone();
                        while !boundary_names.insert(name.clone()) {
                            name = format!("optimized.{name}");
                        }
                        name
                    },
                    port: BlueprintPortRef {
                        instance: vec![root.clone()],
                        port: port.name,
                    },
                })
                .chain(target.scope.environment_boundaries.clone())
                .collect(),
        },
    };
    Ok(BlueprintReductionCandidate {
        blueprint,
        state,
        behavior_context: context,
    })
}

fn validate_alternative(
    request: &BlueprintReductionRequest,
    baseline: &BlueprintReductionCandidate,
    candidate: &BlueprintReductionCandidate,
) -> Result<(), String> {
    if candidate.blueprint.id != request.candidate_revision
        || candidate.state.id != request.candidate_state
        || candidate.blueprint.parents != baseline.blueprint.parents
        || candidate.state.parents != baseline.state.parents
    {
        return Err("alternative must use the selected new IDs and immutable ancestry".into());
    }
    if !request
        .behavior_context
        .same_execution_assumptions(&candidate.behavior_context)
    {
        return Err("candidate comparison cannot change laws or execution assumptions".into());
    }
    if !candidate
        .blueprint
        .behavior_bindings
        .iter()
        .any(|b| b.behavior_type() == request.target.behavior_type())
    {
        return Err("candidate must retain the target behavioral obligation".into());
    }
    if !candidate
        .state
        .assembly
        .instances
        .iter()
        .any(|i| i.revision == candidate.blueprint.id)
    {
        return Err("candidate Assembly must include the candidate Blueprint".into());
    }
    // Changing unknown space to assumed air is not a geometry optimization.
    if candidate.state.assembly.known_regions != baseline.state.assembly.known_regions {
        return Err("alternative must retain the supplied observation coverage".into());
    }
    let supplied = &baseline.state.assembly;
    let explicit: BTreeSet<_> = supplied.blocks.iter().map(|b| b.position).collect();
    if candidate.state.assembly.blocks.iter().any(|b| {
        !explicit.contains(&b.position)
            && !supplied.known_regions.iter().any(|r| {
                let p = b.position;
                p.x >= r.min.x
                    && p.x <= r.max.x
                    && p.y >= r.min.y
                    && p.y <= r.max.y
                    && p.z >= r.min.z
                    && p.z <= r.max.z
            })
    }) {
        return Err("alternative cannot turn unknown space into declared blocks or air".into());
    }
    Ok(())
}

fn consider(
    report: &mut BlueprintReductionReport,
    catalog: &BlueprintCatalog,
    candidate: BlueprintReductionCandidate,
    target: &Target,
    started: Instant,
    budget: BlueprintReductionBudget,
) -> Result<(), String> {
    let (blocks, total_blocks) = target.scope.counts(catalog, &candidate)?;
    report.stats.bindings_examined += 1;
    if has_initial_hold_counterexample(catalog, &candidate, &report.target_type, started, budget) {
        report.stats.screened_counterexamples += 1;
        return Ok(());
    }
    let review = evaluate(catalog, &candidate, fresh_budget(started, budget))?;
    match review.status() {
        CheckStatus::Passed => {
            report.stats.passed += 1;
            if blocks
                < report
                    .best
                    .as_ref()
                    .map_or(report.baseline_blocks, |best| best.occupied_blocks)
            {
                report.best = Some(ReviewedReductionCandidate {
                    candidate,
                    occupied_blocks: blocks,
                    total_occupied_blocks: total_blocks,
                    review: (&review).into(),
                });
            }
        }
        CheckStatus::Failed => report.stats.failed += 1,
        CheckStatus::Undetermined => report.stats.undetermined += 1,
    }
    Ok(())
}

/// A necessary-condition screen, never a passing certificate. Every examined
/// trajectory starts from the declared fresh state, selects one legal input
/// vector, then holds it. Only a closed complete-state cycle refutes the type.
/// Unfinished/error cases proceed to the normal universal/contextual gate.
fn has_initial_hold_counterexample(
    catalog: &BlueprintCatalog,
    candidate: &BlueprintReductionCandidate,
    id: &TypeRevisionId,
    started: Instant,
    budget: BlueprintReductionBudget,
) -> bool {
    let screen = || -> Result<bool, String> {
        let definition = catalog.type_revision(id).ok_or("unknown type")?;
        let TypeContract::RepeatedSettling { relation } = &definition.contract else {
            return Ok(false);
        };
        let mut staged = catalog.clone();
        staged
            .insert_revision(candidate.blueprint.clone())
            .map_err(|e| e.to_string())?;
        staged
            .insert_assembly(candidate.state.clone())
            .map_err(|e| e.to_string())?;
        let instance = candidate
            .state
            .assembly
            .instances
            .iter()
            .find(|i| i.revision == candidate.blueprint.id)
            .ok_or("missing candidate occurrence")?;
        let binding = candidate
            .blueprint
            .behavior_bindings
            .iter()
            .find(|b| b.behavior_type() == id)
            .ok_or("missing target binding")?;
        let selection = BlueprintReductionRequest {
            base_state: candidate.state.id.clone(),
            scope: Default::default(),
            target_instance: vec![instance.instance.clone()],
            target: binding.clone(),
            candidate_revision: candidate.blueprint.id.clone(),
            candidate_state: candidate.state.id.clone(),
            behavior_context: candidate.behavior_context.clone(),
            alternatives: vec![],
        };
        let (target, ports) = target_ports(&staged, &selection)?;
        let outputs = ports
            .iter()
            .skip(target.inputs.len())
            .map(|p| {
                (
                    p.name.clone(),
                    if p.kind == BlueprintPortKind::BlockPower {
                        PhysicalOutput::BlockPower {
                            position: p.position,
                        }
                    } else {
                        PhysicalOutput::Signal {
                            position: p.position,
                        }
                    },
                )
            })
            .collect();
        let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &staged,
            PhysicalBehaviorSelection {
                assembly: candidate.state.id.clone(),
                behavior_type: id.clone(),
                dust_law: candidate.behavior_context.dust_law.clone(),
                torch_law: candidate.behavior_context.torch_law.clone(),
                inputs: target.inputs.into_iter().zip(target.controls).collect(),
                outputs,
                max_electrical_iterations: candidate.behavior_context.max_electrical_iterations,
            },
            candidate.behavior_context.profile,
        )?;
        initial_holds_refute(&model, relation, || time_left(started, budget))
    };
    screen().unwrap_or(false)
}

fn initial_holds_refute<M: BehaviorModel>(
    model: &M,
    relation: &dustroute_library::behavior_type::RepeatedSettling,
    continue_screening: impl Fn() -> bool,
) -> Result<bool, String> {
    for row in &relation.rows {
        let mut state = model.with_inputs(&model.initial_state()?, &row.inputs)?;
        let mut seen = BTreeMap::new();
        let mut outputs = Vec::new();
        for _ in 0..256 {
            if !continue_screening() {
                return Ok(false);
            }
            if let Some(&entry) = seen.get(&state) {
                if outputs[entry..].iter().any(|actual| actual != &row.outputs) {
                    return Ok(true);
                }
                break;
            }
            seen.insert(state.clone(), outputs.len());
            outputs.push(model.outputs(&state)?);
            state = model.step(&state)?;
        }
    }
    Ok(false)
}

/// Reject only a logical contradiction at aliased terminals. No sampled trace,
/// label, delay or assumed function is used as proof or as a type requirement.
fn compatible_aliases(contract: &TypeContract, terminals: &[Terminal], input_count: usize) -> bool {
    if terminals[..input_count].iter().any(|t| t.device_output) {
        return false;
    }
    if terminals[..input_count]
        .iter()
        .collect::<BTreeSet<_>>()
        .len()
        != input_count
    {
        return false;
    }
    let TypeContract::RepeatedSettling { relation } = contract else {
        return true;
    };
    relation.rows.iter().all(|row| {
        let values: Vec<_> = row.inputs.iter().chain(&row.outputs).copied().collect();
        (0..terminals.len())
            .all(|i| (0..i).all(|j| terminals[i] != terminals[j] || values[i] == values[j]))
    })
}

fn next_combination(indices: &mut [usize], count: usize) -> bool {
    for i in (0..indices.len()).rev() {
        if indices[i] < count - indices.len() + i {
            indices[i] += 1;
            for j in i + 1..indices.len() {
                indices[j] = indices[j - 1] + 1;
            }
            return true;
        }
    }
    false
}
fn next_assignment(indices: &mut [usize], count: usize) -> bool {
    for index in indices.iter_mut().rev() {
        *index += 1;
        if *index < count {
            return true;
        }
        *index = 0;
    }
    false
}

// Returns true when search must stop: a smaller passing candidate was found
// at this cardinality, or the shared search budget was exhausted.
fn search_layout(
    catalog: &BlueprintCatalog,
    request: &BlueprintReductionRequest,
    target: &Target,
    report: &mut BlueprintReductionReport,
    blocks: Vec<PositionedBlock>,
    started: Instant,
    budget: BlueprintReductionBudget,
) -> Result<bool, String> {
    if report.stats.layouts_examined >= budget.max_layouts || !time_left(started, budget) {
        report.stop_reason = "search budget exhausted".into();
        return Ok(true);
    }
    report.stats.layouts_examined += 1;
    let mut world = dustroute_translate::World::new();
    for b in &blocks {
        world.set(b.position, b.block.clone());
    }
    if !target
        .controls
        .iter()
        .all(|p| world.kind_at(*p) == BlockKind::Lever)
        || !world.placement_issues().is_empty()
    {
        return Ok(false);
    }
    let options: Vec<_> = world
        .iter()
        .filter(|(position, _)| target.scope.owns(**position))
        .filter_map(|(position, block)| {
            if block.kind == BlockKind::RedstoneWire {
                Some(Terminal {
                    position: *position,
                    block_power: false,
                    device_output: false,
                })
            } else if block.redstone_traits().conducts_weak_power
                || block.redstone_traits().conducts_strong_power
            {
                Some(Terminal {
                    position: *position,
                    block_power: true,
                    device_output: false,
                })
            } else if BlueprintPortKind::DeviceOutput.matches_signal_block(block) {
                Some(Terminal {
                    position: *position,
                    block_power: false,
                    device_output: true,
                })
            } else {
                None
            }
        })
        .collect();
    if options.is_empty() {
        return Ok(false);
    }
    let contract = &catalog
        .type_revision(request.target.behavior_type())
        .expect("checked type")
        .contract;
    let mut assignment = vec![0; target.inputs.len() + target.outputs.len()];
    loop {
        if report.stats.bindings_examined >= budget.max_bindings || !time_left(started, budget) {
            report.stop_reason = "search budget exhausted".into();
            return Ok(true);
        }
        let selected: Vec<_> = assignment.iter().map(|i| options[*i]).collect();
        if compatible_aliases(contract, &selected, target.inputs.len()) {
            let ports = target
                .inputs
                .iter()
                .chain(&target.outputs)
                .enumerate()
                .map(|(i, name)| {
                    selected[i].port(
                        name,
                        if i < target.inputs.len() {
                            PortDirection::Input
                        } else {
                            PortDirection::Output
                        },
                    )
                })
                .collect();
            let candidate = make_candidate(catalog, request, target, ports, blocks.clone())?;
            consider(report, catalog, candidate, target, started, budget)?;
            if report.best.as_ref().is_some_and(|best| {
                best.occupied_blocks == world.iter().filter(|(p, _)| target.scope.owns(**p)).count()
            }) {
                report.stop_reason =
                    "smaller generated layouts exhausted; global minimality not established".into();
                return Ok(true);
            }
        }
        if !next_assignment(&mut assignment, options.len()) {
            break;
        }
    }
    Ok(false)
}

/// Compares arbitrary supplied rewrites, then searches deletion subsets and single-conductor
/// relocations of actual geometry with freely rebound signal ports. Real controls are retained
/// in this generated family; explicit alternatives can relocate them too.
/// All budgets bound computation, never the target circuit's settling time.
/// Results are proposal data only; adoption requires existing fresh review gates.
pub fn reduce_blueprint_blocks(
    catalog: &BlueprintCatalog,
    request: &BlueprintReductionRequest,
    budget: BlueprintReductionBudget,
) -> Result<BlueprintReductionReport, String> {
    let started = Instant::now();
    if catalog.revision(&request.candidate_revision).is_some()
        || catalog.assembly(&request.candidate_state).is_some()
    {
        return Err("optimization needs unused candidate revision and state IDs".into());
    }
    let base = catalog
        .assembly(&request.base_state)
        .ok_or("unknown base Assembly")?;
    let view = base.assembly.inspect(catalog).map_err(|e| e.to_string())?;
    let (target, ports) = target_ports(catalog, request)?;
    let baseline = make_candidate(
        catalog,
        request,
        &target,
        ports,
        base.assembly.blocks.clone(),
    )?;
    let (baseline_blocks, baseline_total_blocks) = target.scope.counts(catalog, &baseline)?;
    let baseline_review = evaluate(catalog, &baseline, fresh_budget(started, budget))?;
    let mut report = BlueprintReductionReport {
        base_state: base.id.clone(),
        target_type: request.target.behavior_type().clone(),
        baseline_blocks,
        baseline_total_blocks,
        cost_scope: request.scope.clone(),
        baseline_scope: "selected_type_in_complete_actual_assembly",
        baseline: (&baseline_review).into(),
        best: None,
        prior_interpretations: view.occurrences.values().cloned().collect(),
        stats: Default::default(),
        stop_reason: "baseline did not pass the selected target type".into(),
        global_minimality_proven: false,
    };
    if baseline_review.status() != CheckStatus::Passed {
        return Ok(report);
    }
    for alternative in &request.alternatives {
        if report.stats.bindings_examined >= budget.max_bindings || !time_left(started, budget) {
            report.stop_reason = "search budget exhausted".into();
            return Ok(report);
        }
        validate_alternative(request, &baseline, alternative)?;
        consider(
            &mut report,
            catalog,
            alternative.clone(),
            &target,
            started,
            budget,
        )?;
    }
    let occupied: Vec<_> = base
        .assembly
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.block.kind != BlockKind::Air && target.scope.owns(b.position))
        .map(|(i, _)| i)
        .collect();
    for size in 0..occupied.len() {
        if size
            >= report
                .best
                .as_ref()
                .map_or(report.baseline_blocks, |b| b.occupied_blocks)
        {
            break;
        }
        let mut subset: Vec<_> = (0..size).collect();
        loop {
            let keep: BTreeSet<_> = subset.iter().map(|i| occupied[*i]).collect();
            let blocks: Vec<_> = base
                .assembly
                .blocks
                .iter()
                .enumerate()
                .map(|(i, b)| PositionedBlock {
                    position: b.position,
                    block: if keep.contains(&i) || !target.scope.owns(b.position) {
                        b.block.clone()
                    } else {
                        Block::new(BlockKind::Air)
                    },
                })
                .collect();
            if search_layout(
                catalog,
                request,
                &target,
                &mut report,
                blocks.clone(),
                started,
                budget,
            )? {
                return Ok(report);
            }
            let mut world = dustroute_translate::World::new();
            for b in &blocks {
                world.set(b.position, b.block.clone());
            }
            if target
                .controls
                .iter()
                .all(|p| world.kind_at(*p) == BlockKind::Lever)
            {
                // Candidate geometry only: conductors can be relocated beside
                // retained signal devices. No gate label or hardcoded function
                // determines whether any position implements the target type.
                let mut destinations = BTreeSet::new();
                for (position, block) in world.iter() {
                    if !matches!(
                        block.kind,
                        BlockKind::RedstoneWire
                            | BlockKind::RedstoneTorch
                            | BlockKind::Lever
                            | BlockKind::RedstoneBlock
                    ) {
                        continue;
                    }
                    for (dx, dy, dz) in [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ] {
                        let (Some(x), Some(y), Some(z)) = (
                            position.x.checked_add(dx),
                            position.y.checked_add(dy),
                            position.z.checked_add(dz),
                        ) else {
                            continue;
                        };
                        let destination = Pos::new(x, y, z);
                        if world.kind_at(destination) == BlockKind::Air
                            && target.scope.owns(destination)
                            && view.block_at(destination).is_some()
                        {
                            destinations.insert(destination);
                        }
                    }
                }
                for (index, block) in blocks.iter().enumerate() {
                    if !target.scope.owns(block.position) {
                        continue;
                    }
                    let traits = block.block.redstone_traits();
                    if !(traits.conducts_weak_power || traits.conducts_strong_power) {
                        continue;
                    }
                    for destination in &destinations {
                        let mut moved = blocks.clone();
                        moved[index].block = Block::new(BlockKind::Air);
                        if let Some(record) = moved.iter_mut().find(|r| r.position == *destination)
                        {
                            record.block = block.block.clone();
                        } else {
                            moved.push(PositionedBlock {
                                position: *destination,
                                block: block.block.clone(),
                            });
                        }
                        if search_layout(
                            catalog,
                            request,
                            &target,
                            &mut report,
                            moved,
                            started,
                            budget,
                        )? {
                            return Ok(report);
                        }
                    }
                }
            }
            if !next_combination(&mut subset, occupied.len()) {
                break;
            }
        }
    }
    report.stop_reason =
        "smaller generated layouts exhausted; global minimality not established".into();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
    use dustroute_translate::behavior_type::verify_repeated_settling;

    enum Example {
        SlowStartup,
        BreaksOnReuse,
        WrongOutput,
    }
    impl BehaviorModel for Example {
        type State = (u16, bool);
        fn initial_state(&self) -> Result<Self::State, String> {
            Ok((0, false))
        }
        fn with_inputs(&self, s: &Self::State, input: &[bool]) -> Result<Self::State, String> {
            let memory = if matches!(self, Self::BreaksOnReuse) && input[0] != s.1 {
                (s.0 + 1).min(2)
            } else {
                s.0
            };
            Ok((memory, input[0]))
        }
        fn step(&self, s: &Self::State) -> Result<Self::State, String> {
            Ok((
                if matches!(self, Self::SlowStartup) {
                    (s.0 + 1).min(300)
                } else {
                    s.0
                },
                s.1,
            ))
        }
        fn outputs(&self, s: &Self::State) -> Result<Vec<bool>, String> {
            Ok(vec![match self {
                Self::SlowStartup if s.0 < 300 => false,
                Self::BreaksOnReuse if s.0 == 2 => false,
                Self::WrongOutput => s.1,
                _ => !s.1,
            }])
        }
    }

    #[test]
    fn screening_never_turns_a_transient_or_fresh_only_success_into_a_type_decision() {
        let relation = RepeatedSettling {
            inputs: vec!["a".into()],
            outputs: vec!["out".into()],
            rows: [false, true]
                .into_iter()
                .map(|b| BooleanRow {
                    inputs: vec![b],
                    outputs: vec![!b],
                })
                .collect(),
        };
        let definition = TypeRevision {
            id: TypeRevisionId::new("screen.type.v1").unwrap(),
            name: "Repeated relation".into(),
            contract: TypeContract::RepeatedSettling {
                relation: relation.clone(),
            },
        };
        // 256 incorrect startup samples are not a deadline on the circuit.
        assert!(!initial_holds_refute(&Example::SlowStartup, &relation, || true).unwrap());
        assert_eq!(
            verify_repeated_settling(
                &definition,
                &Example::SlowStartup,
                BehaviorBudget::default()
            )
            .status,
            CheckStatus::Passed
        );
        // Each fresh held-input case passes, but reuse still fails the full type.
        assert!(!initial_holds_refute(&Example::BreaksOnReuse, &relation, || true).unwrap());
        assert_eq!(
            verify_repeated_settling(
                &definition,
                &Example::BreaksOnReuse,
                BehaviorBudget::default()
            )
            .status,
            CheckStatus::Failed
        );
        assert!(initial_holds_refute(&Example::WrongOutput, &relation, || true).unwrap());
        assert!(!initial_holds_refute(&Example::WrongOutput, &relation, || false).unwrap());
    }
}
