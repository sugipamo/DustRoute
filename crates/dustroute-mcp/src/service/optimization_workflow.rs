//! Proposal computation operates on an immutable captured circuit. It has no
//! bridge, player-selection or MCP router capability and cannot write a world.
use super::*;

pub(super) struct OptimizationWorkflow<'a> {
    pub policy: &'a McpPolicy,
    pub state_store: &'a PlanStateStore,
    pub app: &'a DustRouteService,
    pub operations: &'a OperationRegistry,
}
impl OptimizationWorkflow<'_> {
    async fn store_repair_plan(
        &self,
        id: uuid::Uuid,
        plan: StoredRepairPlan,
    ) -> Result<(), String> {
        repair_workflow::RepairPlans(self.state_store).save(id, &plan)
    }
    pub(super) async fn propose_macro(
        &self,
        circuit_id: uuid::Uuid,
        circuit: StoredCircuit,
        component_id: String,
        contract: OptimizationContract,
    ) -> Value {
        let (_, world) = match dustroute_translate::snapshot::world_from_snapshot(&circuit.snapshot)
        {
            Ok(result) => (circuit.snapshot.clone(), result),
            Err(error) => {
                return workflow_error(McpErrorCode::SerializationFailed, error.to_string(), false);
            }
        };
        let staged = self.app.analyze_physical(
            &world,
            ReverseRequest::new(circuit.bounds)
                .with_truth_table(16)
                .with_observation_complete(circuit.complete),
        );
        let Some(model) = staged.reverse.functional_network.as_ref() else {
            return workflow_error(
                McpErrorCode::InvalidState,
                "a complete inferred functional network is required for macro optimization",
                false,
            );
        };
        let candidates = find_builtin_verified_macro_replacements(
            model,
            "java",
            "1.21.11",
            ObservedMacroMetrics::from_world(&world),
        );
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.component_id.as_str() == component_id)
        else {
            return workflow_error(
                McpErrorCode::NotFound,
                "component_id is not a current verified macro replacement candidate",
                false,
            );
        };
        let boundary = extract_model_boundary_with_context(model, &world, &staged.reverse.analysis);
        let reserved = boundary
            .iter()
            .filter_map(|port| port.driver_position)
            .collect::<BTreeSet<_>>();
        let boundary_positions = boundary
            .iter()
            .map(|port| port.position)
            .collect::<BTreeSet<_>>();
        let replaceable = world
            .positions()
            .filter(|position| !boundary_positions.contains(position))
            .collect::<BTreeSet<_>>();
        let mut placement =
            match plan_macro_replacement_with_reserved(candidate, &boundary, &reserved) {
                Ok(plan) => plan,
                Err(error) => {
                    return workflow_error(
                        McpErrorCode::InvalidState,
                        format!("macro placement is unavailable: {error:?}"),
                        false,
                    );
                }
            };
        let structural = validate_macro_structure(&placement, &world, &replaceable);
        if !structural.valid() {
            return workflow_error(
                McpErrorCode::VerificationFailed,
                "macro replacement failed structural validation",
                false,
            );
        }
        placement.verification.structural = ContextualVerificationState::Passed;
        let materialized = match materialize_macro_replacement_in_known_regions(
            &placement,
            &world,
            &[dustroute_translate::world::Region::new(
                circuit.snapshot.min,
                circuit.snapshot.max,
            )],
            &replaceable,
            14,
        ) {
            Ok(materialized) => materialized,
            Err(error) => {
                return workflow_error(
                    McpErrorCode::VerificationFailed,
                    format!("macro replacement could not be materialized: {error:?}"),
                    false,
                );
            }
        };
        if let Err(error) = self
            .policy
            .validate_placement_size(materialized.patch.changes.len())
        {
            return workflow_error(McpErrorCode::PermissionDenied, error.to_string(), false);
        }
        let steady =
            verify_macro_steady_state(&model.truth_table, &world, &materialized.world, 8, 64);
        let transitions = (steady.state == ContextualVerificationState::Passed).then(|| {
            verify_macro_transitions(
                &model.truth_table,
                &world,
                &materialized.world,
                64,
                contract.timing.settle_deadline_redstone_ticks.max(20),
                4,
            )
        });
        let strength_verification = verify_boundary_strengths(
            &world,
            &materialized.world,
            &model.truth_table,
            &boundary_positions.iter().copied().collect::<Vec<_>>(),
            64,
        );
        let assessment = assess_macro_contract(
            contract,
            &structural,
            Some(&steady),
            transitions.as_ref(),
            materialized.patch.changes.len(),
            Some(strength_verification.is_ok()),
        );
        let contract_satisfied = assessment.satisfied();
        let changed_positions = materialized
            .patch
            .changes
            .iter()
            .map(|change| change.pos)
            .collect::<BTreeSet<_>>();
        let preserved_boundary = circuit
            .snapshot
            .blocks
            .iter()
            .filter(|record| {
                boundary_positions.contains(&record.pos)
                    || (!changed_positions.contains(&record.pos)
                        && world
                            .get(record.pos)
                            .is_some_and(|block| block.kind.is_redstone_related()))
            })
            .map(boundary_block_record)
            .collect::<Vec<_>>();
        let operation_id = uuid::Uuid::new_v4();
        if let Err(error) = self
            .store_repair_plan(
                operation_id,
                StoredRepairPlan {
                    patch: materialized.patch.clone(),
                    dimension: circuit.dimension.clone(),
                    analysis_bounds: circuit.bounds,
                    fragments_before: staged.reverse.analysis.scene.fragments.len(),
                    baseline_truth_table: Some(model.truth_table.clone()),
                    lifecycle: RepairLifecycle::Draft,
                    contract_satisfied,
                    preserved_boundary,
                },
            )
            .await
        {
            return workflow_error(McpErrorCode::Internal, error, false);
        }
        self.operations
            .record_completed(
                operation_id,
                OperationKind::OptimizationProposal,
                json!({
                    "circuit_id": circuit_id,
                    "component_id": component_id,
                    "patch": materialized.patch,
                    "contract": optimization_contract_json(contract),
                    "contract_assessment": contract_assessment_json(&assessment),
                }),
            )
            .await;
        json!({
            "schema_version": OPTIMIZATION_SCHEMA_V1,
            "ok": true,
            "circuit_id": circuit_id,
            "operation_id": operation_id,
            "optimization_kind": "macro_replacement",
            "component_id": component_id,
            "name": candidate.name,
            "contract": optimization_contract_json(contract),
            "contract_assessment": contract_assessment_json(&assessment),
            "placement": {
                "origin": placement.placed.origin,
                "rotation_y": format!("{:?}", placement.placed.rotation).to_lowercase(),
                "route_length": placement.total_route_length,
            },
            "metrics": {
                "changed_blocks": materialized.patch.changes.len(),
                "added_supports": materialized.added_supports.len(),
                "inserted_repeaters": materialized.inserted_repeaters.len(),
            },
            "verification": {
                "structural": "passed",
                "steady_state": format!("{:?}", steady.state).to_lowercase(),
                "transition_cases": transitions.as_ref().map(|report| report.cases.len()),
                "transition_differences": transitions.as_ref().map(|report| report.differing_cases),
                "boundary_strength": match strength_verification {
                    Ok(()) => json!({ "state": "passed" }),
                    Err(reason) => json!({ "state": "failed", "reason": reason }),
                },
            },
            "patch": materialized.patch,
            "next_step": if contract_satisfied {
                "call show_operation, obtain explicit confirmation, then invoke_operation(confirm=true)"
            } else {
                "do not invoke; inspect the failed or unavailable contract categories"
            },
        })
    }
    pub(super) async fn propose_wire(
        &self,
        circuit_id: uuid::Uuid,
        circuit: StoredCircuit,
        focus: dustroute_translate::world_reverse::RegionBounds,
        objective: String,
        contract: OptimizationContract,
        search_budget: PhysicalOptimizationSearchBudget,
    ) -> Value {
        if !circuit.bounds.contains(focus.min) || !circuit.bounds.contains(focus.max) {
            return workflow_error(
                McpErrorCode::InvalidArgument,
                "focus must be fully contained by the immutable circuit snapshot",
                false,
            );
        }
        if let Err(error) = self.policy.validate_region(focus) {
            return workflow_error(McpErrorCode::PermissionDenied, error.to_string(), false);
        }
        let world = match world_from_snapshot_for_service(&circuit.snapshot) {
            Ok(world) => world,
            Err(error) => return workflow_error(McpErrorCode::SerializationFailed, error, false),
        };
        let optimization = match optimize_physical_wire_path_with_budget(
            &world,
            focus,
            contract.analog.preserve_strength,
            search_budget,
        ) {
            Ok(optimization) => optimization,
            Err(error) => {
                return workflow_error(
                    McpErrorCode::InvalidState,
                    format!("no safe physical wire optimization: {error:?}"),
                    false,
                );
            }
        };
        if let Err(error) = self
            .policy
            .validate_placement_size(optimization.patch.changes.len())
        {
            return workflow_error(McpErrorCode::PermissionDenied, error.to_string(), false);
        }
        if optimization.patch.changes.len() > contract.mutation.maximum_changed_blocks {
            return workflow_error(
                McpErrorCode::VerificationFailed,
                format!(
                    "{} changed blocks exceeds contract maximum {}",
                    optimization.patch.changes.len(),
                    contract.mutation.maximum_changed_blocks
                ),
                false,
            );
        }
        let mut optimized_world = match optimization.patch.apply_virtual(&world) {
            Ok(world) => world,
            Err(error) => {
                return workflow_error(McpErrorCode::InvalidState, error.to_string(), false);
            }
        };
        dustroute_translate::wire::update_wire_shapes(&mut optimized_world);
        let before =
            dustroute_translate::world_reverse::analyze_world_region(&world, circuit.bounds);
        let after = dustroute_translate::world_reverse::analyze_world_region(
            &optimized_world,
            circuit.bounds,
        );
        let before_temporal = before.scene.temporal_assessment();
        let after_temporal = after.scene.temporal_assessment();
        if before_temporal.requirement != after_temporal.requirement {
            return workflow_error(
                McpErrorCode::VerificationFailed,
                "optimization changed the circuit temporal requirement",
                false,
            );
        }
        let before_diagnostic = dustroute_translate::diagnostic::diagnose_scene(
            &before.scene,
            circuit.target,
            circuit.complete,
        );
        let after_diagnostic = dustroute_translate::diagnostic::diagnose_scene(
            &after.scene,
            circuit.target,
            circuit.complete,
        );
        if !before.unsupported.is_empty()
            || !after.unsupported.is_empty()
            || after_diagnostic.counts.probable_faults > before_diagnostic.counts.probable_faults
            || after_diagnostic.counts.unsupported > before_diagnostic.counts.unsupported
        {
            return workflow_error(
                McpErrorCode::VerificationFailed,
                "optimization cannot be applied while unsupported physics or a new diagnostic fault is present",
                false,
            );
        }
        let before_truth =
            dustroute_translate::world_reverse::infer_truth_table(&world, &before, 8, 64).ok();
        let after_truth =
            dustroute_translate::world_reverse::infer_truth_table(&optimized_world, &after, 8, 64)
                .ok();
        let semantic = match (&before_truth, &after_truth) {
            (Some(before), Some(after)) => {
                let comparison =
                    dustroute_translate::world_reverse::compare_truth_tables(before, after);
                if !comparison.comparable || comparison.fitness_penalty != 0 {
                    return workflow_error(
                        McpErrorCode::VerificationFailed,
                        "optimization changed the inferred truth table",
                        false,
                    );
                }
                json!({ "available": true, "equivalent": true, "comparison": comparison })
            }
            _ => json!({
                "available": false,
                "reason": "truth-table inference was unavailable; the plan remains preview-only physical-path optimization"
            }),
        };
        let steady_state =
            before_truth
                .as_ref()
                .zip(after_truth.as_ref())
                .map(|(before_truth, after_truth)| {
                    let comparison = dustroute_translate::world_reverse::compare_truth_tables(
                        before_truth,
                        after_truth,
                    );
                    let differing_assignments = before_truth
                        .rows
                        .iter()
                        .zip(&after_truth.rows)
                        .filter(|(before, after)| before.outputs != after.outputs)
                        .map(|(before, _)| before.inputs.clone())
                        .collect();
                    MacroSteadyStateReport {
                        state: if comparison.comparable && comparison.differing_bits == 0 {
                            ContextualVerificationState::Passed
                        } else {
                            ContextualVerificationState::Failed
                        },
                        comparison: Some(comparison),
                        input_mapping: (0..before_truth.inputs.len()).collect(),
                        output_mapping: (0..before_truth.outputs.len()).collect(),
                        differing_assignments,
                        reason: None,
                    }
                });
        let transitions = before_truth.as_ref().zip(after_truth.as_ref()).and_then(
            |(before_truth, after_truth)| {
                steady_state
                    .as_ref()
                    .is_some_and(|report| report.state == ContextualVerificationState::Passed)
                    .then(|| {
                        verify_world_transitions(
                            &world,
                            before_truth,
                            &optimized_world,
                            after_truth,
                            64,
                            contract.timing.settle_deadline_redstone_ticks.max(20),
                            4,
                        )
                    })
            },
        );
        let structural = MacroStructuralReport::default();
        let strength_verification = before_truth.as_ref().map(|truth| {
            verify_boundary_strengths(
                &world,
                &optimized_world,
                truth,
                &optimization.fixed_endpoints,
                64,
            )
        });
        let used_temporary_expansion = optimization
            .phases
            .iter()
            .any(|phase| phase.connector_growth > 0);
        let mut contract_assessment = assess_macro_contract(
            contract,
            &structural,
            steady_state.as_ref(),
            transitions.as_ref(),
            optimization.patch.changes.len(),
            strength_verification.as_ref().map(Result::is_ok),
        );
        if !contract.mutation.allow_temporary_expansion && used_temporary_expansion {
            contract_assessment.mutation = ContractCheck {
                state: ContractCheckState::Failed,
                reason_codes: vec!["temporary_expansion_forbidden".to_owned()],
                reasons: vec![
                    "the search used temporary connector expansion forbidden by the contract"
                        .to_owned(),
                ],
            };
        }
        let contract_satisfied = contract_assessment.satisfied();
        let changed_positions = optimization
            .patch
            .changes
            .iter()
            .map(|change| change.pos)
            .collect::<BTreeSet<_>>();
        let fixed_endpoints = optimization
            .fixed_endpoints
            .into_iter()
            .collect::<BTreeSet<_>>();
        let preserved_boundary = circuit
            .snapshot
            .blocks
            .iter()
            .filter(|record| {
                fixed_endpoints.contains(&record.pos)
                    || (!changed_positions.contains(&record.pos)
                        && world
                            .get(record.pos)
                            .is_some_and(|block| block.kind.is_redstone_related()))
            })
            .map(boundary_block_record)
            .collect::<Vec<_>>();
        let operation_id = uuid::Uuid::new_v4();
        let phase_trace = optimization
            .phases
            .iter()
            .map(|phase| {
                json!({
                    "name": phase.name,
                    "accepted": phase.accepted,
                    "before": {
                        "bounding_volume": phase.before.bounding_volume,
                        "occupied_blocks": phase.before.occupied_blocks,
                        "connector_length": phase.before.connector_length
                    },
                    "after": {
                        "bounding_volume": phase.after.bounding_volume,
                        "occupied_blocks": phase.after.occupied_blocks,
                        "connector_length": phase.after.connector_length
                    },
                    "connector_growth": phase.connector_growth
                })
            })
            .collect::<Vec<_>>();
        let fragments_before = before.scene.fragments.len();
        if let Err(error) = self
            .store_repair_plan(
                operation_id,
                StoredRepairPlan {
                    patch: optimization.patch.clone(),
                    dimension: circuit.dimension.clone(),
                    analysis_bounds: circuit.bounds,
                    fragments_before,
                    baseline_truth_table: before_truth,
                    lifecycle: RepairLifecycle::Draft,
                    contract_satisfied,
                    preserved_boundary: preserved_boundary.clone(),
                },
            )
            .await
        {
            return workflow_error(McpErrorCode::Internal, error, false);
        }
        self.operations
            .record_completed(
                operation_id,
                OperationKind::OptimizationProposal,
                json!({
                    "circuit_id": circuit_id,
                    "focus": bounds_json(focus),
                    "patch": optimization.patch,
                    "semantic_verification": semantic,
                    "contract": optimization_contract_json(contract),
                    "contract_assessment": contract_assessment_json(&contract_assessment)
                }),
            )
            .await;
        json!({
            "schema_version": OPTIMIZATION_SCHEMA_V1,
            "ok": true,
            "circuit_id": circuit_id,
            "operation_id": operation_id,
            "objective": objective,
            "contract": optimization_contract_json(contract),
            "contract_assessment": contract_assessment_json(&contract_assessment),
            "focus": bounds_json(focus),
            "outside_focus_fixed": true,
            "preserved_boundary_blocks": preserved_boundary.len(),
            "fixed_endpoints": optimization.fixed_endpoints,
            "metrics": {
                "wire_blocks_before": optimization.wire_blocks_before,
                "wire_blocks_after": optimization.wire_blocks_after,
                "path_length_before": optimization.path_length_before,
                "path_length_after": optimization.path_length_after,
                "changed_blocks": optimization.patch.changes.len()
            },
            "search": {
                "budget": {
                    "max_expansions": search_budget.max_expansions,
                    "max_candidates": search_budget.max_candidates,
                    "max_millis": search_budget.max_millis,
                },
                "expansions": optimization.search.expansions,
                "candidates": optimization.search.candidates,
                "truncated": optimization.search.truncated,
                "stop_reason": optimization.search.stop_reason,
            },
            "phase_trace": phase_trace,
            "planning_policy": {
                "temporary_connector_growth_is_internal_only": true,
                "temporary_connector_growth_budget": optimization.path_length_before / 2,
                "final_global_improvement_required": true
            },
            "patch": optimization.patch,
            "verification": {
                "diagnostics_not_worse": true,
                "temporal_requirement_preserved": true,
                "temporal_requirement": after_temporal.requirement,
                "semantic": semantic,
                "steady_state": steady_state.as_ref().map(|report| json!({
                    "state": format!("{:?}", report.state).to_lowercase(),
                    "differing_assignments": report.differing_assignments,
                    "reason": report.reason,
                })),
                "transitions": transitions.as_ref().map(|report| json!({
                    "state": format!("{:?}", report.state).to_lowercase(),
                    "case_count": report.cases.len(),
                    "differing_cases": report.differing_cases,
                    "reason": report.reason,
                })),
                "boundary_strength": strength_verification.as_ref().map(|result| match result {
                    Ok(()) => json!({ "state": "passed" }),
                    Err(reason) => json!({ "state": "failed", "reason": reason }),
                })
            },
            "next_step": if contract_satisfied {
                "call show_operation, explain the fixed focus and verified contract, obtain explicit confirmation, then call invoke_operation(confirm=true)"
            } else {
                "do not invoke this operation; satisfy every failed or unavailable contract category first"
            }
        })
    }
}
