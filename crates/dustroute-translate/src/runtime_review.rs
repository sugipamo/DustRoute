//! Diagnostic whole-realization review in the explicit moving-world context.
//! No deserialization, source mutation or promotion/adoption authority is exposed.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use dustroute_library::PortDirection;
use dustroute_library::assembly::{Assembly, AssemblyPortRef, AssemblyView};
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintPortKind, InstancePath, TypeContract, TypeRevisionId,
};
use dustroute_library::execution_context::check_law_requirements;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::time::runtime::RuntimeView;
use dustroute_minecraft::{Block, BlockKind, Pos, RotationY};
use serde::Serialize;

use crate::behavior_type::{BehaviorBudget, BehaviorModel, BehaviorTypeReport};
use crate::promotion::{CheckKind, CheckResult, CheckStatus, OccurrenceReview, aggregate};
use crate::runtime_behavior::{RuntimeBehaviorModel, RuntimeBehaviorState, fresh_runtime};

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeAssemblyReport {
    pub assembly: Assembly,
    pub context: RuntimeBehaviorContext,
    pub occurrences: BTreeMap<InstancePath, OccurrenceReview>,
    pub arrangement: Vec<CheckResult>,
    pub behavior: Vec<RuntimeBindingReport>,
}
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeBindingReport {
    pub instance: InstancePath,
    pub report: BehaviorTypeReport,
}
impl RuntimeAssemblyReport {
    pub fn status(&self) -> CheckStatus {
        aggregate(
            self.occurrences
                .values()
                .map(OccurrenceReview::status)
                .chain(self.arrangement.iter().map(|c| c.status)),
        )
    }
    fn record(&mut self, path: &InstancePath, check: CheckResult) {
        self.occurrences
            .get_mut(path)
            .expect("indexed occurrence")
            .checks
            .push(check);
    }
}
fn check(kind: CheckKind, status: CheckStatus, detail: impl Into<String>) -> CheckResult {
    CheckResult {
        kind,
        status,
        detail: detail.into(),
    }
}

enum Expected {
    Known,
    Kind(BlockKind),
    Exact(Box<Block>),
    Interface(BlueprintPortKind),
}
struct Requirement {
    path: InstancePath,
    position: Pos,
    kind: CheckKind,
    label: String,
    expected: Expected,
    evidence: Option<CheckResult>,
}
#[derive(Default)]
struct Monitor {
    requirements: Vec<Requirement>,
}
impl Monitor {
    fn add(
        &mut self,
        path: &InstancePath,
        position: Pos,
        kind: CheckKind,
        label: String,
        expected: Expected,
    ) {
        self.requirements.push(Requirement {
            path: path.clone(),
            position,
            kind,
            label,
            expected,
            evidence: None,
        });
    }
    fn observe(&mut self, mut lookup: impl FnMut(Pos) -> Result<Block, String>, at: &str) {
        for requirement in &mut self.requirements {
            if requirement
                .evidence
                .as_ref()
                .is_some_and(|c| c.status == CheckStatus::Failed)
            {
                continue;
            }
            let (status, detail) = match lookup(requirement.position) {
                Err(reason) => (CheckStatus::Undetermined, reason),
                Ok(actual) => {
                    let satisfied = match &requirement.expected {
                        Expected::Known => true,
                        Expected::Kind(kind) => actual.kind == *kind,
                        Expected::Exact(block) => actual == **block,
                        Expected::Interface(kind) => {
                            *kind == BlueprintPortKind::BlockState
                                || kind.matches_signal_block(&actual)
                        }
                    };
                    if satisfied {
                        continue;
                    }
                    (CheckStatus::Failed, format!("actual block {actual:?}"))
                }
            };
            if requirement.evidence.is_none() || status == CheckStatus::Failed {
                requirement.evidence = Some(check(
                    requirement.kind,
                    status,
                    format!(
                        "{} at {:?}; {at}; {detail}",
                        requirement.label, requirement.position
                    ),
                ));
            }
        }
    }
    fn runtime(&mut self, view: RuntimeView<'_>) {
        self.observe(
            |p| {
                view.observe_location(p)
                    .map(|s| s.block().clone())
                    .map_err(|e| e.to_string())
            },
            &format!("runtime {:?}", view.time()),
        );
    }
    fn finish(self, closed: bool, report: &mut RuntimeAssemblyReport) {
        for r in self.requirements {
            let evidence = r.evidence.unwrap_or_else(|| {
                let (status, scope) = if closed {
                    (CheckStatus::Passed, "checked throughout the closed reachable graph and every committed microstep")
                } else {
                    (CheckStatus::Undetermined, "no contradiction observed, but the physical graph was not closed")
                };
                check(r.kind, status, format!("{} at {:?}; {scope}", r.label, r.position))
            });
            report.record(&r.path, evidence);
        }
    }
}

fn translated(origin: Pos, offset: Pos, rotation: RotationY) -> Result<Pos, BlueprintError> {
    let d = rotation
        .checked_pos(offset)
        .ok_or(BlueprintError::CoordinateOverflow)?;
    Ok(Pos::new(
        origin
            .x
            .checked_add(d.x)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        origin
            .y
            .checked_add(d.y)
            .ok_or(BlueprintError::CoordinateOverflow)?,
        origin
            .z
            .checked_add(d.z)
            .ok_or(BlueprintError::CoordinateOverflow)?,
    ))
}
fn static_requirement(
    catalog: &BlueprintCatalog,
    view: &AssemblyView,
    monitor: &mut Monitor,
    report: &mut RuntimeAssemblyReport,
    target: (&InstancePath, CheckKind),
    reference: &AssemblyPortRef,
    id: &TypeRevisionId,
) -> Result<(), BlueprintError> {
    let (path, kind) = target;
    let (port, rotation) = view.resolved_port(reference)?;
    let definition = catalog
        .type_revision(id)
        .ok_or_else(|| BlueprintError::UnknownType(id.clone()))?;
    let label = format!("{reference:?} requires {id}");
    match &definition.contract {
        TypeContract::Signal { port_kind } => {
            let status = if port.kind == *port_kind {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            };
            report.record(path, check(kind, status, label));
        }
        TypeContract::BlockKind { block_kind } => {
            monitor.add(
                path,
                port.position,
                kind,
                label,
                Expected::Kind(*block_kind),
            );
        }
        TypeContract::BlockPattern { blocks } => {
            for record in blocks {
                let position = translated(port.position, record.position, rotation)?;
                let Some(block) = rotation.checked_block(&record.block) else {
                    report.record(
                        path,
                        check(
                            kind,
                            CheckStatus::Undetermined,
                            format!("{label}: unsupported pattern rotation"),
                        ),
                    );
                    continue;
                };
                monitor.add(
                    path,
                    position,
                    kind,
                    label.clone(),
                    Expected::Exact(Box::new(block)),
                );
            }
        }
        _ => {
            let detail = format!(
                "{label}: contextual producer behavior across a declared route is not supported by this location-only review"
            );
            report.record(path, check(kind, CheckStatus::Undetermined, detail));
        }
    }
    Ok(())
}

// Evidence accumulation is deliberately outside physical state. The callback
// cannot cancel a mutation, alter a successor or make a parent pass repair a
// child. It records only diagnostics from this single fresh exploration.
struct Audited<'a> {
    model: &'a RuntimeBehaviorModel,
    monitor: &'a RefCell<Monitor>,
}
impl BehaviorModel for Audited<'_> {
    type State = RuntimeBehaviorState;
    fn initial_state(&self) -> Result<Self::State, String> {
        self.model.initial_state()
    }
    fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
        self.model
            .drive_observed(state, inputs, &mut |v| self.monitor.borrow_mut().runtime(v))
    }
    fn step(&self, state: &Self::State) -> Result<Self::State, String> {
        Ok(self.step_with_observations(state)?.0)
    }
    fn outputs(&self, state: &Self::State) -> Result<Vec<bool>, String> {
        self.model.outputs(state)
    }
    fn step_with_observations(
        &self,
        state: &Self::State,
    ) -> Result<(Self::State, Vec<Vec<bool>>), String> {
        self.model
            .advance_observed(state, &mut |v| self.monitor.borrow_mut().runtime(v))
    }
}

/// All retained occurrences and unclassified physical blocks stay in the
/// context. Supported behavior and static obligations are independent checks;
/// known child failures survive parent passes, unknowns and budget exhaustion.
pub fn review_assembly_in_runtime_context(
    catalog: &BlueprintCatalog,
    assembly: &Assembly,
    context: &RuntimeBehaviorContext,
    mut budget: BehaviorBudget,
) -> Result<RuntimeAssemblyReport, BlueprintError> {
    let start = Instant::now();
    let view = assembly.inspect(catalog)?;
    let mut report = RuntimeAssemblyReport {
        assembly: assembly.clone(),
        context: context.clone(),
        occurrences: view
            .occurrences
            .iter()
            .map(|(path, o)| {
                (
                    path.clone(),
                    OccurrenceReview {
                        revision: o.revision.clone(),
                        checks: vec![],
                    },
                )
            })
            .collect(),
        arrangement: vec![],
        behavior: vec![],
    };
    let mut monitor = Monitor::default();
    for (position, claims) in &view.source_claims {
        for (path, block) in claims {
            monitor.add(
                path,
                *position,
                CheckKind::Placement,
                if block.kind == BlockKind::Air {
                    "explicit source Air"
                } else {
                    "known claimed position; source differences are not equality requirements"
                }
                .into(),
                if block.kind == BlockKind::Air {
                    Expected::Kind(BlockKind::Air)
                } else {
                    Expected::Known
                },
            );
        }
    }
    let actual_connections = assembly
        .connections
        .iter()
        .map(|edge| view.connection_key(edge))
        .collect::<Result<BTreeSet<_>, _>>()?;
    for edge in &assembly.connections {
        let key = view.connection_key(edge)?;
        let (source, _) = view.resolved_port(&edge.source)?;
        let (sink, _) = view.resolved_port(&edge.sink)?;
        let incompatible = source.direction != PortDirection::Output
            || sink.direction != PortDirection::Input
            || ((source.kind == BlueprintPortKind::BlockState)
                != (sink.kind == BlueprintPortKind::BlockState));
        let result = check(
            CheckKind::Connection,
            if incompatible {
                CheckStatus::Failed
            } else {
                CheckStatus::Undetermined
            },
            format!(
                "{key:?}: {}",
                if incompatible {
                    "incompatible physical interfaces"
                } else {
                    "moving-world route validation is not implemented"
                }
            ),
        );
        for path in [&edge.source.instance, &edge.sink.instance] {
            report.record(path, result.clone());
        }
        report.arrangement.push(result);
    }
    for (path, occurrence) in &view.occurrences {
        let source = catalog
            .revision(&occurrence.revision)
            .expect("indexed source");
        if !source.required_laws.is_empty() {
            let result = match check_law_requirements(
                catalog,
                &context.execution_context(),
                &source.required_laws,
            ) {
                Ok(()) => check(
                    CheckKind::PhysicalLaw,
                    CheckStatus::Passed,
                    "declared laws selected by this world context",
                ),
                Err(e) => check(CheckKind::PhysicalLaw, CheckStatus::Failed, e),
            };
            report.record(path, result);
        }
        for port in &source.ports {
            let reference = AssemblyPortRef {
                instance: path.clone(),
                port: port.name.clone(),
            };
            let (actual, _) = view.resolved_port(&reference)?;
            monitor.add(
                path,
                actual.position,
                CheckKind::Port,
                format!("fixed terminal {reference:?}"),
                Expected::Interface(actual.kind),
            );
            if port.direction == PortDirection::Input && !port.required_source_types.is_empty() {
                let canonical = view.canonical_port_ref(&reference)?;
                let mut found = false;
                for edge in &assembly.connections {
                    if view.canonical_port_ref(&edge.sink)? != canonical {
                        continue;
                    }
                    found = true;
                    for id in &port.required_source_types {
                        static_requirement(
                            catalog,
                            &view,
                            &mut monitor,
                            &mut report,
                            (path, CheckKind::SourceRequirement),
                            &edge.source,
                            id,
                        )?;
                    }
                }
                if !found {
                    report.record(
                        path,
                        check(
                            CheckKind::SourceRequirement,
                            CheckStatus::Undetermined,
                            format!("no producer supplied for {reference:?}"),
                        ),
                    );
                }
            }
        }
        for binding in &source.static_type_bindings {
            static_requirement(
                catalog,
                &view,
                &mut monitor,
                &mut report,
                (path, CheckKind::StaticType),
                &AssemblyPortRef {
                    instance: path.clone(),
                    port: binding.port.clone(),
                },
                &binding.type_revision,
            )?;
        }
        for edge in &source.connections {
            let prefix = |r: &AssemblyPortRef| AssemblyPortRef {
                instance: path.iter().chain(&r.instance).cloned().collect(),
                port: r.port.clone(),
            };
            let key = (
                view.canonical_port_ref(&prefix(&edge.source))?.clone(),
                view.canonical_port_ref(&prefix(&edge.sink))?.clone(),
            );
            report.record(
                path,
                check(
                    CheckKind::SourceConnection,
                    if actual_connections.contains(&key) {
                        CheckStatus::Undetermined
                    } else {
                        CheckStatus::Failed
                    },
                    format!(
                        "retained source connection {key:?}; route must be checked independently"
                    ),
                ),
            );
        }
    }
    monitor.observe(
        |p| {
            view.block_at(p)
                .ok_or_else(|| "unknown initial coordinate".into())
        },
        "initial Assembly",
    );
    let runtime = fresh_runtime(catalog, assembly, context);
    match &runtime {
        Ok(run) => {
            monitor.runtime(run.view());
            report.arrangement.push(check(
                CheckKind::Placement,
                CheckStatus::Passed,
                "complete actual world accepted by the native initial-placement gate",
            ));
        }
        Err(e) => report.arrangement.push(check(
            CheckKind::Placement,
            CheckStatus::Undetermined,
            e.clone(),
        )),
    }
    let monitor = RefCell::new(monitor);
    let mut closed = false;
    let elapsed_limit = budget.max_elapsed;
    for (path, occurrence) in &view.occurrences {
        let source = catalog
            .revision(&occurrence.revision)
            .expect("indexed source");
        for binding in &source.behavior_bindings {
            budget.max_elapsed = elapsed_limit.saturating_sub(start.elapsed());
            let result = if budget.max_elapsed.is_zero()
                || budget.max_states == 0
                || budget.max_steps == 0
            {
                Err("shared runtime review budget exhausted".into())
            } else {
                RuntimeBehaviorModel::from_fresh_assembly(catalog, assembly, path, binding, context)
            };
            match result {
                Err(e) => report.record(
                    path,
                    check(
                        CheckKind::Behavior,
                        CheckStatus::Undetermined,
                        format!("{}: {e}", binding.behavior_type()),
                    ),
                ),
                Ok(model) => {
                    let behavior = model.verify_observed(
                        &Audited {
                            model: &model,
                            monitor: &monitor,
                        },
                        budget,
                    );
                    closed |= behavior.graph_closed;
                    budget.max_states = budget.max_states.saturating_sub(behavior.reachable_states);
                    budget.max_steps = budget.max_steps.saturating_sub(behavior.evaluated_steps);
                    report.record(
                        path,
                        check(
                            CheckKind::Behavior,
                            behavior.status,
                            format!(
                                "{path:?}: {}",
                                serde_json::to_string(&behavior)
                                    .expect("serializable behavior diagnostics")
                            ),
                        ),
                    );
                    report.behavior.push(RuntimeBindingReport {
                        instance: path.clone(),
                        report: behavior,
                    });
                }
            }
        }
    }
    if !closed {
        report.arrangement.push(check(CheckKind::Behavior,CheckStatus::Undetermined,"no complete reachable physical graph; snapshots and successful prefixes cannot certify whole-realization behavior"));
    }
    monitor.into_inner().finish(closed, &mut report);
    Ok(report)
}
