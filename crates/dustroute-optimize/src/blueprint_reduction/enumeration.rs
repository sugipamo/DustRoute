//! Enumerate a physical family first, then check its declared behavior. No NOT
//! name, truth table shortcut, or static pair of block states supplies evidence.
use super::*;

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TorchSupportEnumerationRequest {
    /// Component scope is required. Candidate IDs become prefixes for distinct
    /// orientation candidates; no records are published by enumeration.
    pub search: BlueprintReductionRequest,
    /// An actual conducting body block retained as the attachment support.
    /// This is a candidate-family anchor, not a coordinate in the behavioral type.
    pub support_position: Pos,
}

#[derive(Clone, Debug, Serialize)]
pub struct EnumeratedTorchSupport {
    pub placement: &'static str,
    pub status: CheckStatus,
    pub candidate: Option<ReviewedReductionCandidate>,
    pub issue: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TorchSupportEnumerationReport {
    pub target_type: TypeRevisionId,
    pub candidates: Vec<EnumeratedTorchSupport>,
    pub placements_examined: usize,
    pub family_exhausted: bool,
    pub global_minimality_proven: bool,
}

pub fn enumerate_torch_supports(
    catalog: &BlueprintCatalog,
    request: &TorchSupportEnumerationRequest,
    budget: BlueprintReductionBudget,
) -> Result<TorchSupportEnumerationReport, String> {
    let started = Instant::now();
    let search = &request.search;
    if !search.alternatives.is_empty() {
        return Err("layout enumeration does not consume alternative candidates".into());
    }
    let (target, _) = target_ports(catalog, search)?;
    if !target.scope.component || target.inputs.len() != 1 || target.outputs.len() != 1 {
        return Err(
            "torch/support enumeration needs component scope and one input/one output".into(),
        );
    }
    let base = catalog
        .assembly(&search.base_state)
        .ok_or("unknown Assembly")?;
    let view = base.assembly.inspect(catalog).map_err(|e| e.to_string())?;
    let support = view
        .block_at(request.support_position)
        .ok_or("unknown support")?;
    if !target.scope.owns(request.support_position)
        || !BlueprintPortKind::BlockPower.matches_signal_block(&support)
    {
        return Err("the selected attachment support must be a conducting body block".into());
    }
    let mut report = TorchSupportEnumerationReport {
        target_type: search.target.behavior_type().clone(),
        candidates: vec![],
        placements_examined: 0,
        family_exhausted: false,
        global_minimality_proven: false,
    };
    for (name, offset, facing) in [
        ("top", Pos::new(0, 1, 0), dustroute_translate::Facing::Up),
        ("east", Pos::new(1, 0, 0), dustroute_translate::Facing::East),
        (
            "west",
            Pos::new(-1, 0, 0),
            dustroute_translate::Facing::West,
        ),
        (
            "south",
            Pos::new(0, 0, 1),
            dustroute_translate::Facing::South,
        ),
        (
            "north",
            Pos::new(0, 0, -1),
            dustroute_translate::Facing::North,
        ),
    ] {
        if report.placements_examined >= budget.max_layouts.min(budget.max_bindings)
            || !time_left(started, budget)
        {
            return Ok(report);
        }
        report.placements_examined += 1;
        let build = || -> Result<(CheckStatus, ReviewedReductionCandidate), String> {
            let anchor = request.support_position;
            let torch_position = Pos::new(
                anchor
                    .x
                    .checked_add(offset.x)
                    .ok_or("coordinate overflow")?,
                anchor
                    .y
                    .checked_add(offset.y)
                    .ok_or("coordinate overflow")?,
                anchor
                    .z
                    .checked_add(offset.z)
                    .ok_or("coordinate overflow")?,
            );
            if !target.scope.owns(torch_position) {
                return Err("torch placement collides with fixed external equipment".into());
            }
            if view.block_at(torch_position).is_none() {
                return Err("torch placement is outside known physical space".into());
            }
            let mut variant = search.clone();
            variant.candidate_revision =
                BlueprintRevisionId::new(format!("{}.{}", search.candidate_revision, name))
                    .map_err(str::to_owned)?;
            variant.candidate_state =
                AssemblyRevisionId::new(format!("{}.{}", search.candidate_state, name))
                    .map_err(str::to_owned)?;
            if catalog.revision(&variant.candidate_revision).is_some()
                || catalog.assembly(&variant.candidate_state).is_some()
            {
                return Err("enumeration needs unused candidate IDs".into());
            }
            let mut blocks: BTreeMap<_, _> = base
                .assembly
                .blocks
                .iter()
                .map(|b| {
                    (
                        b.position,
                        if target.scope.owns(b.position) {
                            Block::new(BlockKind::Air)
                        } else {
                            b.block.clone()
                        },
                    )
                })
                .collect();
            blocks.insert(anchor, support.clone());
            let mut torch = Block::new(BlockKind::RedstoneTorch);
            torch.facing = Some(facing);
            torch.support_offset = Some(Pos::new(-offset.x, -offset.y, -offset.z));
            torch.powered = Some(true);
            blocks.insert(torch_position, torch);
            let ports = vec![
                Terminal {
                    position: anchor,
                    block_power: true,
                    device_output: false,
                }
                .port(&target.inputs[0], PortDirection::Input),
                Terminal {
                    position: torch_position,
                    block_power: false,
                    device_output: true,
                }
                .port(&target.outputs[0], PortDirection::Output),
            ];
            let candidate = make_candidate(
                catalog,
                &variant,
                &target,
                ports,
                blocks
                    .into_iter()
                    .map(|(position, block)| PositionedBlock { position, block })
                    .collect(),
            )?;
            let (occupied_blocks, total_occupied_blocks) =
                target.scope.counts(catalog, &candidate)?;
            let review = evaluate(catalog, &candidate, fresh_budget(started, budget))?;
            Ok((
                review.status(),
                ReviewedReductionCandidate {
                    candidate,
                    occupied_blocks,
                    total_occupied_blocks,
                    review: (&review).into(),
                },
            ))
        };
        match build() {
            Ok((status, candidate)) => report.candidates.push(EnumeratedTorchSupport {
                placement: name,
                status,
                candidate: Some(candidate),
                issue: None,
            }),
            Err(issue) => report.candidates.push(EnumeratedTorchSupport {
                placement: name,
                status: CheckStatus::Undetermined,
                candidate: None,
                issue: Some(issue),
            }),
        }
    }
    report.family_exhausted = true;
    Ok(report)
}
