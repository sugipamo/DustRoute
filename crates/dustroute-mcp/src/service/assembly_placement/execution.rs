//! Consume one reviewed attempt, verify each write and persist partial progress.
use super::*;

impl AssemblyService<'_> {
    pub(in crate::service) async fn mutate_assembly_construction(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
    ) -> Value {
        let result: Result<Value,String> = async {
            if !confirm { return Err("confirm=true is required".into()); }
            self.policy.authorize_mutation().map_err(|e|e.to_string())?;
            let _guard = self.mutation_lock.lock().await;
            let plan = self.owned_assembly_plan(id,None).await?;
            if undo && plan.instance().is_some() { return Err("use a new reviewed removal or placement for this instance operation".into()); }
            if (undo && plan.state != PistonPlacementState::Applied) || (!undo && (plan.state != PistonPlacementState::Planned || !plan.previewed || plan.expires_at <= Instant::now())) {
                return Err("construction needs an unused preview; undo needs a verified application".into());
            }
            let removal = undo || plan.is_removal();
            let instance_id = plan.instance().map_or(id, |(instance_id,_)|instance_id);
            let registry = RegistryLock::acquire(self.state_store)?;
            let basis = self.construction_basis(&plan.player,plan.assembly_id.clone()).await?;
            if !plan.source_identity.matches(&basis) { return Err("adopted source or context changed; create a new construction plan".into()); }
            let transform = plan.transform;
            let reconstruct_from = plan.reconstruction().map(|r| r.baseline.clone());
            let remove_from = plan.operating_removal().map(|r| r.baseline.clone());
            let (proof, steps) = tokio::task::spawn_blocking(move || {
                let proof = proof_from_basis(&basis,transform)?;
                let steps = if let Some(baseline) = reconstruct_from {
                    proof.reconstruction_steps(&baseline)?
                } else if let Some(baseline) = remove_from {
                    proof.operating_removal(&baseline)?.steps
                } else { proof.steps(removal).to_vec() };
                Ok::<_,String>((proof,steps))
            }).await.map_err(|e|e.to_string())??;
            let bounds = proof.bounds();
            self.policy.authorize_dimension(&plan.dimension).map_err(|e|e.to_string())?;
            self.policy.validate_region(bounds).map_err(|e|e.to_string())?;
            self.policy.validate_placement_size(steps.len()).map_err(|e|e.to_string())?;
            if proof.assembly() != plan.proof.assembly() || proof.context() != plan.proof.context()
                || steps != plan.steps(undo) {
                return Err("fresh construction/removal differs from the preview; replan".into());
            }
            let status = self.bridge.status().await.map_err(|e|e.to_string())?;
            server_contract(&status,&plan.dimension)?;
            plan.target.check(&status)?;
            let mut record = if removal || plan.reconstruction().is_some() {
                let record = registry.get(instance_id,&plan.player)?;
                if (removal && record.state != InstanceState::Applied)
                    || record.state == InstanceState::Removed
                    || plan.instance().is_some_and(|(_,revision)|record.revision != revision) {
                    return Err("placed Assembly record changed or is not applied; reobserve and replan".into());
                }
                if record.source_identity != plan.source_identity || record.target != plan.target
                    || &record.assembly != proof.assembly() || &record.context != proof.context()
                    || record.transform != plan.transform {
                    return Err("placed Assembly pins differ from the removal plan".into());
                }
                ValidatedAssemblyPlacement::matches(&record.expected,proof.settled(),&status.version)?;
                let observation = self.observe_instance(&record).await;
                if let Some(reconstruction) = plan.reconstruction() {
                    let actual = observation::stable_baseline(&observation)?;
                    ValidatedAssemblyPlacement::matches(&actual,&reconstruction.baseline,&status.version)?;
                } else if let Some(operating) = plan.operating_removal() {
                    let actual = observation::stable_baseline(&observation)?;
                    ValidatedAssemblyPlacement::matches(&actual,&operating.baseline,&status.version)?;
                } else if !observation.matches_reference() { return Err(format!("fresh removal observation refused: {observation}")); }
                record
            } else {
                PlacedAssembly { schema:PlacedAssembly::schema(),instance_id,revision:0,player:plan.player.clone(),assembly_id:plan.assembly_id.clone(),
                    source_identity:plan.source_identity.clone(),transform:plan.transform,target:plan.target.clone(),assembly:proof.assembly().clone(),
                    context:proof.context().clone(),expected:proof.settled().clone(),state:InstanceState::NeedsInspection,attempts:vec![],
                    last_observation:None,updated_at_unix_ms:now_ms()? }
            };
            let baseline_readback = self.bridge.scan_region_validated(bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?.into_record();
            let baseline = baseline_readback.snapshot;
            if let Some(reconstruction) = plan.reconstruction() {
                ValidatedAssemblyPlacement::matches(&baseline,&reconstruction.baseline,&status.version)?;
            } else if let Some(operating) = plan.operating_removal() {
                ValidatedAssemblyPlacement::matches(&baseline,&operating.baseline,&status.version)?;
            } else { proof.validate_before(&baseline,&status.version,removal)?; }
            if !undo && plan.expires_at <= Instant::now() { return Err("construction expired during validation".into()); }
            self.plans.table::<assembly_placement::StoredAssemblyPlacement>().lock().await.get_mut(&id).ok_or("construction missing")?.state = PistonPlacementState::NeedsInspection;
            record.state = InstanceState::NeedsInspection;
            record.last_observation = None;
            record.attempts.push(Attempt { operation_id:id,removal,reconstruction:plan.reconstruction().cloned(),operating_removal:plan.operating_removal().cloned(),verified_steps:0,total_steps:steps.len(),started_at_unix_ms:now_ms()?,finished_at_unix_ms:None,error:None,readbacks:vec![baseline_readback.readback] });
            registry.save(&mut record)?;
            let mut completed = 0;
            let mut run: Result<(),String> = async {
                let mut expected = baseline;
                for step in &steps {
                    let status = self.bridge.status().await.map_err(|e|e.to_string())?;
                    server_contract(&status,&plan.dimension)?;
                    plan.target.check(&status)?;
                    let before_readback = self.bridge.scan_region_validated(bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?.into_record();
                    let before = before_readback.snapshot;
                    ValidatedAssemblyPlacement::matches(&before,&expected,&status.version)?;
                    record.attempts.last_mut().ok_or("missing durable attempt")?.readbacks.push(before_readback.readback);
                    registry.save(&mut record)?;
                    self.bridge.write_blocks(&[CommandWrite { pos:step.position, state:step.state.parse()? }],&plan.dimension).await.map_err(|e|e.to_string())?;
                    // The bridge accepts at most 200 ticks per request. A
                    // modeled chain may need longer before whole-region readback.
                    let mut remaining = step.wait_ticks;
                    while remaining > 0 {
                        let ticks = remaining.min(200) as u16;
                        self.bridge.wait_ticks(ticks,&plan.dimension).await.map_err(|e|e.to_string())?;
                        remaining -= u64::from(ticks);
                    }
                    let after_readback = self.bridge.scan_region_validated(bounds.min,bounds.max,&plan.dimension).await.map_err(|e|e.to_string())?.into_record();
                    let after = after_readback.snapshot;
                    record.attempts.last_mut().ok_or("missing durable attempt")?.readbacks.push(after_readback.readback);
                    let after_status = self.bridge.status().await.map_err(|e|e.to_string())?;
                    plan.target.check(&after_status)?;
                    ValidatedAssemblyPlacement::matches(&after,&step.expected,&after_status.version)?;
                    expected = step.expected.clone();
                    completed += 1;
                    record.attempts.last_mut().ok_or("missing durable attempt")?.verified_steps = completed;
                    registry.save(&mut record)?;
                }
                proof.validate_after(&expected,&status.version,removal)
            }.await;
            record.state = if run.is_ok() { if removal {InstanceState::Removed}else{InstanceState::Applied} } else {InstanceState::NeedsInspection};
            let attempt = record.attempts.last_mut().ok_or("missing durable attempt")?;
            attempt.finished_at_unix_ms = Some(now_ms()?);
            attempt.error = run.as_ref().err().cloned();
            if let Err(error) = registry.save(&mut record) { run = Err(error); }
            if run.is_ok() {
                self.plans.table::<assembly_placement::StoredAssemblyPlacement>().lock().await.get_mut(&id).ok_or("construction missing")?.state = if removal {PistonPlacementState::Undone} else {PistonPlacementState::Applied};
            }
            let response = json!({"ok":run.is_ok(),"operation_id":id,"instance_id":instance_id,"record_revision":record.revision,"undo":removal,"kind":plan.kind(),"verified_steps":completed,"total_steps":steps.len(),
                "reconstruction_conditions":plan.reconstruction().map(|_|reconstruction::conditions()),
                "status":if run.is_ok(){"verified"}else{"needs_inspection"},"error":run.err(),"retry_allowed":false,"automatic_rollback":false,"bounds":bounds_json(bounds)});
            self.operations.record_completed(id,if removal {OperationKind::PlacementUndo}else{OperationKind::PlacementApply},response.clone()).await;
            Ok(response)
        }.await;
        result.unwrap_or_else(|e| json!({"ok":false,"error":e}))
    }
}
