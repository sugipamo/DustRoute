//! Fresh contextual checks. Callers supply assumptions, never a saved pass.
use std::collections::BTreeMap;
use std::time::Instant;

use dustroute_library::assembly::{Assembly, AssemblyPortRef, AssemblyView};
use dustroute_library::behavior_type::PhysicalBehaviorContext;
use dustroute_library::blueprint::{
    BehaviorBinding, BlueprintCatalog, BlueprintPort, BlueprintPortKind, InstancePath,
    TypeContract, TypeRevisionId,
};

use crate::abstract_behavior::{HISTORY_ABSTRACTION_METHOD, verify_abstract_repeated_settling};
use crate::behavior_type::BehaviorBudget;
use crate::finite_burst::verify_finite_burst;
use crate::periodic::verify_periodic;
use crate::physical_behavior::abstract_history::PhysicalHistoryAbstraction;
use crate::physical_behavior::{PhysicalExecutionModel, PhysicalOutput};
use crate::promotion::{CheckKind, CheckResult, CheckStatus};
use crate::world::Pos;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BoundBehavior {
    // Name -> (actual lever, observed terminal). Coordinates are in the Assembly.
    inputs: BTreeMap<String, (Pos, PhysicalOutput)>,
    outputs: BTreeMap<String, PhysicalOutput>,
}

pub(crate) struct BehaviorReview<'a> {
    catalog: &'a BlueprintCatalog,
    assembly: &'a Assembly,
    context: Option<&'a PhysicalBehaviorContext>,
    budget: BehaviorBudget,
    started: Instant,
    // Reuse only within one exact environment and law context. All ports and
    // drivers participate; matching one output is insufficient for a relation.
    cache: BTreeMap<(TypeRevisionId, BoundBehavior), CheckResult>,
    pub(crate) checks: Vec<CheckResult>,
}

impl<'a> BehaviorReview<'a> {
    pub(crate) fn new(
        catalog: &'a BlueprintCatalog,
        assembly: &'a Assembly,
        context: Option<&'a PhysicalBehaviorContext>,
        budget: BehaviorBudget,
    ) -> Self {
        Self {
            catalog,
            assembly,
            context,
            budget,
            started: Instant::now(),
            cache: BTreeMap::new(),
            checks: vec![],
        }
    }

    fn unknown(id: &TypeRevisionId, detail: impl std::fmt::Display) -> CheckResult {
        CheckResult {
            kind: CheckKind::Behavior,
            status: CheckStatus::Undetermined,
            detail: format!("{id}: {detail}"),
            evidence: Some(Box::new(crate::review_diagnostics::CheckEvidence {
                type_revision: Some(id.clone()),
                ..Default::default()
            })),
        }
    }

    fn observation(port: &BlueprintPort) -> Result<PhysicalOutput, String> {
        match port.kind {
            BlueprintPortKind::Wire | BlueprintPortKind::DeviceOutput => {
                Ok(PhysicalOutput::Signal {
                    position: port.position,
                })
            }
            BlueprintPortKind::BlockPower => Ok(PhysicalOutput::BlockPower {
                position: port.position,
            }),
            BlueprintPortKind::BlockState => {
                Err("behavior observation needs a signal terminal".into())
            }
        }
    }

    fn bind(
        &self,
        binding: &BehaviorBinding,
        path: &InstancePath,
        view: &AssemblyView,
    ) -> Result<BoundBehavior, String> {
        let resolve = |name: &str| {
            view.resolved_port(&AssemblyPortRef {
                instance: path.clone(),
                port: name.into(),
            })
            .map(|(port, _)| port)
            .map_err(|error| error.to_string())
        };
        match binding {
            BehaviorBinding::Observed {..} => Err("explicit observations need a supported runtime behavior context; legacy fixed-geometry profiles cannot certify them".into()),
            BehaviorBinding::Autonomous {
                behavior_type,
                output_port,
            } => self.bind_autonomous(behavior_type, &resolve(output_port)?),
            BehaviorBinding::RepeatedSettling {
                inputs, outputs, ..
            } => {
                let context = self
                    .context
                    .ok_or("behavioral obligation needs an explicit execution context")?;
                let mut result = BoundBehavior {
                    inputs: BTreeMap::new(),
                    outputs: BTreeMap::new(),
                };
                for (name, terminal) in inputs {
                    let port = resolve(terminal)?;
                    let drivers: Vec<_> = context
                        .input_drivers
                        .iter()
                        .filter(|driver| {
                            driver.port_position == port.position && driver.port_kind == port.kind
                        })
                        .collect();
                    if drivers.len() != 1 {
                        return Err(format!(
                            "input terminal {terminal} at {:?} needs exactly one explicit physical input driver",
                            port.position
                        ));
                    }
                    result.inputs.insert(
                        name.clone(),
                        (drivers[0].lever_position, Self::observation(&port)?),
                    );
                }
                for (name, terminal) in outputs {
                    result
                        .outputs
                        .insert(name.clone(), Self::observation(&resolve(terminal)?)?);
                }
                Ok(result)
            }
        }
    }

    fn bind_autonomous(
        &self,
        id: &TypeRevisionId,
        port: &BlueprintPort,
    ) -> Result<BoundBehavior, String> {
        let output = match &self
            .catalog
            .type_revision(id)
            .ok_or("unknown behavior type")?
            .contract
        {
            TypeContract::Periodic { requirement } => &requirement.output,
            TypeContract::FiniteBurst { requirement } => &requirement.output,
            _ => {
                return Err(
                    "a repeated-settling requirement needs a complete declared port mapping".into(),
                );
            }
        };
        Ok(BoundBehavior {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(output.clone(), Self::observation(port)?)]),
        })
    }

    pub(crate) fn check_binding(
        &mut self,
        binding: &BehaviorBinding,
        path: &InstancePath,
        view: &AssemblyView,
    ) -> CheckResult {
        let mut result = match self.bind(binding, path, view) {
            Ok(bound) => self.check_bound(binding.behavior_type(), bound),
            Err(error) => Self::unknown(binding.behavior_type(), error),
        };
        result.evidence.as_mut().expect("behavior evidence").binding = Some(binding.clone());
        self.checks.push(result.clone());
        result
    }

    pub(crate) fn check_source(
        &mut self,
        id: &TypeRevisionId,
        source: &AssemblyPortRef,
        view: &AssemblyView,
    ) -> CheckResult {
        let bound = (|| -> Result<BoundBehavior, String> {
            let (port, _) = view.resolved_port(source).map_err(|e| e.to_string())?;
            if !matches!(
                self.catalog
                    .type_revision(id)
                    .expect("validated type")
                    .contract,
                TypeContract::RepeatedSettling { .. }
            ) {
                return self.bind_autonomous(id, &port);
            }
            let canonical = view.canonical_port_ref(source).map_err(|e| e.to_string())?;
            let mut selected = None;
            // Parent aliases can expose the same producer terminal. Resolve the
            // complete declared relation rather than inventing the other ports.
            for (path, occurrence) in &view.occurrences {
                let revision = self
                    .catalog
                    .revision(&occurrence.revision)
                    .expect("indexed occurrence");
                for binding in &revision.behavior_bindings {
                    if binding.behavior_type() != id {
                        continue;
                    }
                    let covers = revision
                        .ports
                        .iter()
                        .filter(|port| binding.covers_output(&port.name))
                        .any(|port| {
                            view.canonical_port_ref(&AssemblyPortRef {
                                instance: path.clone(),
                                port: port.name.clone(),
                            })
                            .is_ok_and(|reference| reference == canonical)
                        });
                    if !covers {
                        continue;
                    }
                    let bound = self.bind(binding, path, view)?;
                    if selected.as_ref().is_some_and(|previous| previous != &bound) {
                        return Err("producer has ambiguous repeated-settling port mappings".into());
                    }
                    selected = Some(bound);
                }
            }
            selected.ok_or(
                "producer has no declared repeated-settling port mapping for the selected output"
                    .into(),
            )
        })();
        let mut result = match bound {
            Ok(bound) => self.check_bound(id, bound),
            Err(error) => Self::unknown(id, error),
        };
        result.evidence.as_mut().expect("behavior evidence").port = Some(source.clone());
        self.checks.push(result.clone());
        result
    }

    fn check_bound(&mut self, id: &TypeRevisionId, bound: BoundBehavior) -> CheckResult {
        let Some(context) = self.context else {
            return Self::unknown(
                id,
                "behavioral obligation needs an explicit execution context",
            );
        };
        let key = (id.clone(), bound.clone());
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let mut result = Self::unknown(id, "shared behavioral review budget exhausted");
        if self.started.elapsed() >= self.budget.max_elapsed {
            return result;
        }
        let definition = self.catalog.type_revision(id).expect("validated type");
        let inputs = bound
            .inputs
            .iter()
            .map(|(name, (lever, _))| (name.clone(), *lever))
            .collect();
        let outcome = PhysicalExecutionModel::from_fresh_assembly(
            self.catalog,
            self.assembly,
            definition,
            context,
            &inputs,
            &bound.outputs,
        );
        let scope = format!(
            "{id}; bindings={bound:?}; context={}",
            serde_json::to_string(context).expect("serializable context")
        );
        match outcome {
            Err(error) => result.detail = format!("{scope}: {error}"),
            Ok(model) => {
                let budget = BehaviorBudget {
                    max_elapsed: self
                        .budget
                        .max_elapsed
                        .saturating_sub(self.started.elapsed()),
                    ..self.budget
                };
                let diagnostics = match &definition.contract {
                    TypeContract::Periodic { .. } => {
                        let report = verify_periodic(definition, &model, budget);
                        Ok((
                            report.status,
                            report.reachable_states,
                            report.evaluated_steps,
                            serde_json::to_value(&report).expect("serializable diagnostics"),
                        ))
                    }
                    TypeContract::FiniteBurst { .. } => {
                        let report = verify_finite_burst(definition, &model, budget);
                        Ok((
                            report.status,
                            report.reachable_states,
                            report.evaluated_steps,
                            serde_json::to_value(&report).expect("serializable diagnostics"),
                        ))
                    }
                    TypeContract::RepeatedSettling { relation } => {
                        let ports = relation
                            .inputs
                            .iter()
                            .map(|name| bound.inputs[name].1)
                            .collect();
                        PhysicalHistoryAbstraction::with_input_ports(&model, ports).map(
                            |abstract_model| {
                                let report = verify_abstract_repeated_settling(
                                    definition,
                                    &abstract_model,
                                    budget,
                                );
                                (
                                    report.status,
                                    report.abstract_states,
                                    report.evaluated_transitions,
                                    serde_json::json!({"abstraction_method":HISTORY_ABSTRACTION_METHOD,"report":report}),
                                )
                            },
                        )
                    }
                    _ => Err("selected type does not describe supported behavior".into()),
                };
                match diagnostics {
                    Ok((status, states, steps, diagnostics)) => {
                        self.budget.max_states = self.budget.max_states.saturating_sub(states);
                        self.budget.max_steps = self.budget.max_steps.saturating_sub(steps);
                        result.status = status;
                        result.detail = format!("{scope}: {diagnostics}");
                        result
                            .evidence
                            .as_mut()
                            .expect("behavior type evidence")
                            .behavior = Some(diagnostics);
                    }
                    Err(error) => result.detail = format!("{scope}: {error}"),
                }
            }
        }
        self.cache.insert(key, result.clone());
        result
    }
}
