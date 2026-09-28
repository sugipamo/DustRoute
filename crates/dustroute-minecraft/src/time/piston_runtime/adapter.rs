use super::geometry::*;
use super::notifications::*;
use super::*;
use crate::piston_motion_law::{PistonBlockEvent, builtin_laws};
use crate::{
    Block, BlockKind, BlockMove, DeltaCause, ObservationClassification, PistonState, Pos,
    WorldValidationIssue,
};

pub struct ElectricalPistonAdapter;

impl RuntimeAdapter for ElectricalPistonAdapter {
    type Payload = PistonEvent;
    const REVISION: &'static str = ELECTRICAL_PROFILE;

    fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
        super::electrical::validate_scope(view, true)?;
        validate_initial(view)
    }

    fn handle(
        event: &Invocation<PistonEvent>,
        view: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
        let out = handle(event, view)?;
        if let Some(delta) = &out.delta {
            let mut world = view.world().clone();
            delta
                .apply(&mut world)
                .map_err(|e| RuntimeError::Invalid(e.to_string()))?;
            let mut staged_carriers = view.staged_carriers.clone();
            for effect in &out.carriers {
                match effect {
                    CarrierEffect::Stage { position, block } => {
                        staged_carriers.insert(*position, (**block).clone());
                    }
                    CarrierEffect::Install { position, .. } => {
                        staged_carriers.remove(position);
                    }
                    _ => {}
                }
            }
            super::electrical::validate_scope(
                RuntimeView {
                    world: &world,
                    staged_carriers: &staged_carriers,
                    ..view
                },
                false,
            )?;
        }
        Ok(out)
    }
}

fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
    // This is a new adapter gate, not a changed meaning of ValidatedWorld.
    let issues = view
        .world()
        .placement_issues_with_lookup(|p| view.block(p).ok());
    for issue in issues {
        if !matches!(
            issue,
            WorldValidationIssue::UnsupportedPlacement {
                kind: BlockKind::Piston | BlockKind::PistonHead,
                ..
            }
        ) {
            return Err(unsupported(format!(
                "unsupported initial placement: {issue:?}"
            )));
        }
    }
    for (pos, block) in view.world().iter() {
        if block.observation_classification == ObservationClassification::Coarse
            || block.requires_live_observation()
        {
            return Err(unsupported(format!(
                "unsupported initial evidence at {pos:?}"
            )));
        }
        if crate::device_program::program(block).is_some() {
            continue;
        }
        match block.kind {
            BlockKind::Piston => {
                let dir = facing(block)?;
                let Some(current) = block.piston_state.filter(|s| s.is_stable()) else {
                    return Err(unsupported(
                        "initial piston state must be explicit and stable",
                    ));
                };
                if let Some(name) = block.observed_name.as_deref() {
                    let name = name.strip_prefix("minecraft:").unwrap_or(name);
                    if !matches!(name, "piston" | "sticky_piston")
                        || block.observed_properties.get("facing").map(String::as_str)
                            != Some(facing_name(dir))
                        || (name == "sticky_piston")
                            != (crate::piston_variant(block) == crate::PistonVariant::Sticky)
                        || block
                            .observed_properties
                            .get("extended")
                            .and_then(|v| v.parse::<bool>().ok())
                            != Some(current.is_extended())
                    {
                        return Err(unsupported(
                            "incomplete or contradictory piston observation",
                        ));
                    }
                }
                let front = along(*pos, dir, 1)?;
                if current == PistonState::Extended {
                    let h = view.block(front)?;
                    if h.kind != BlockKind::PistonHead || !head_supported(view, front, &h)? {
                        return Err(unsupported(
                            "extended piston needs its matching stable head",
                        ));
                    }
                }
                powered(view, *pos, block)?;
            }
            BlockKind::PistonHead => {
                if let Some(name) = block.observed_name.as_deref() {
                    let metadata = block
                        .piston_head
                        .as_ref()
                        .ok_or_else(|| unsupported("head metadata is missing"))?;
                    if name.strip_prefix("minecraft:").unwrap_or(name) != "piston_head"
                        || block.observed_properties.get("facing").map(String::as_str)
                            != Some(facing_name(metadata.facing))
                        || block.observed_properties.get("type").map(String::as_str)
                            != Some(if metadata.variant == crate::PistonVariant::Sticky {
                                "sticky"
                            } else {
                                "normal"
                            })
                        || block
                            .observed_properties
                            .get("short")
                            .and_then(|v| v.parse::<bool>().ok())
                            != Some(metadata.short)
                    {
                        return Err(unsupported("incomplete or contradictory head observation"));
                    }
                }
                if !head_supported(view, *pos, block)? {
                    return Err(unsupported("orphaned initial piston head"));
                }
            }
            BlockKind::Air
            | BlockKind::Solid
            | BlockKind::Transparent
            | BlockKind::RedstoneWire
            | BlockKind::Lever
            | BlockKind::RedstoneBlock
            | BlockKind::Repeater => {}
            other => {
                return Err(unsupported(format!(
                    "block {other:?} has no adapter in this execution profile"
                )));
            }
        }
    }
    Ok(())
}

fn handle(
    event: &Invocation<PistonEvent>,
    view: RuntimeView<'_>,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let pos = event.call.target;
    match &event.call.payload {
        PistonEvent::Motion { plan } => super::movement::step(view, pos, plan),
        PistonEvent::Device {
            callback,
            source,
            captured,
        } => super::devices::start(view, pos, *callback, *source, captured.as_deref()),
        PistonEvent::DeviceContinue { run } => super::devices::step(view, pos, *run.clone()),
        PistonEvent::DeviceAfterArrival { block } => {
            let mut jobs = if view.block(pos)? == **block {
                adjacent_jobs(view, pos, false, true)?
            } else {
                VecDeque::new()
            };
            jobs.push_back(NeighborJob {
                target: pos,
                source: pos,
                shape: false,
            });
            Ok(RuntimeOutcome {
                callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
                ..Default::default()
            })
        }
        PistonEvent::ElectricalInstall { block } => super::command::install(view, pos, block),
        PistonEvent::ElectricalPreprocess { block, side } => {
            super::command::preprocess(view, pos, block, *side)
        }
        PistonEvent::ElectricalWrite { block } => super::command::write(view, pos, block),
        PistonEvent::ElectricalRemoveShapes { block } => {
            super::command::remove_shapes(view, pos, block)
        }
        PistonEvent::ElectricalCommandNeighbors => Ok(RuntimeOutcome {
            continuation: crate::device_program::program(&view.block(pos)?)
                .filter(|p| p.definition().comparator_output.is_some())
                .map(|_| PistonEvent::NotifyAnalogReaders { next_side: 0 }),
            callbacks: vec![call(
                pos,
                PistonEvent::Notify {
                    jobs: adjacent_jobs(view, pos, false, false)?,
                },
            )],
            ..Default::default()
        }),
        PistonEvent::ElectricalAdded => super::command::added(view, pos),
        PistonEvent::ElectricalAfterAdded { block } => {
            super::command::after_added(view, pos, block)
        }
        PistonEvent::ElectricalRemove => super::command::remove(view, pos),
        PistonEvent::ElectricalRemoved { block } => super::command::removed(view, pos, block),
        PistonEvent::ElectricalRemovedWireUpdate { block } => {
            super::command::removed_wire_update(view, pos, block)
        }
        PistonEvent::Initialize => {
            let jobs = view
                .world()
                .iter()
                .filter(|(_, b)| {
                    matches!(
                        b.kind,
                        BlockKind::Piston | BlockKind::PistonHead | BlockKind::RedstoneWire
                    ) || crate::device_program::program(b)
                        .is_some_and(|p| p.definition.initial_neighbor_update)
                })
                .map(|(p, _)| NeighborJob {
                    target: *p,
                    source: *p,
                    shape: false,
                })
                .collect();
            Ok(RuntimeOutcome {
                continuation: Some(PistonEvent::Notify { jobs }),
                ..RuntimeOutcome::default()
            })
        }
        PistonEvent::Input { powered } => {
            if view.block(pos)?.kind != BlockKind::Lever {
                return Err(unsupported("external input must operate an actual lever"));
            }
            let delta = crate::redstone_input_delta(view.world(), pos, *powered)
                .map_err(|e| unsupported(e.to_string()))?;
            let callbacks = if delta.is_some() {
                vec![call(
                    pos,
                    PistonEvent::Notify {
                        jobs: super::electrical::input_jobs(view, pos)?,
                    },
                )]
            } else {
                Vec::new()
            };
            Ok(RuntimeOutcome {
                delta,
                callbacks,
                ..RuntimeOutcome::default()
            })
        }
        PistonEvent::Notify { jobs } => notify(view, jobs.clone()),
        PistonEvent::NotifyAnalogReaders { next_side } => analog_readers(view, pos, *next_side),
        PistonEvent::Block {
            event: action,
            facing,
        } => begin(view, pos, *action, *facing),
        PistonEvent::Arm { positions, after } => {
            let mut queued = Vec::new();
            let delay = builtin_laws()
                .geometry(false, false, view.time().section, 0)
                .expect("fixed facts")
                .first_tick_delay;
            for target in positions {
                let carrier = view
                    .carrier(*target)
                    .ok_or(RuntimeError::CarrierConflict(*target))?
                    .id;
                queued.push(QueueRequest::CarrierTick {
                    game_tick: next_tick(view.time().game_tick, delay)?,
                    carrier,
                    call: call(*target, PistonEvent::CarrierTick),
                });
            }
            Ok(RuntimeOutcome {
                queued,
                continuation: after.as_deref().cloned(),
                ..RuntimeOutcome::default()
            })
        }
        PistonEvent::ExtendBody { body } => {
            let delta = delta(
                view,
                [(pos, state((**body).clone(), true))],
                Vec::new(),
                DeltaCause::PistonExtend { piston: pos },
            )?;
            Ok(RuntimeOutcome {
                delta: Some(delta),
                callbacks: vec![notification(view, pos, &[pos], false)?],
                ..RuntimeOutcome::default()
            })
        }
        PistonEvent::RetractBody {
            body,
            event: action,
            restored_facing,
        } => retract_body(view, pos, body, *action, *restored_facing),
        PistonEvent::RetractPayload {
            body,
            event: action,
        } => retract_payload(view, pos, body, *action),
        PistonEvent::ForceFinish { carrier } => {
            if !view.carrier(pos).is_some_and(|c| c.id == *carrier) {
                return Ok(RuntimeOutcome::default());
            }
            finish_or_advance(view, pos, true)
        }
        PistonEvent::CarrierTick => finish_or_advance(view, pos, false),
    }
}

fn no_attached_components(
    view: RuntimeView<'_>,
    changes: &[Pos],
    retracting_body: Option<Pos>,
) -> Result<(), RuntimeError> {
    for (p, b) in view.world().iter() {
        if b.support_pos(*p)
            .is_some_and(|support| changes.contains(&support))
        {
            // A source body retracts in place. Its back-face attachment receives
            // no shape callback until the stable body is restored; no component
            // is transported or destroyed. Ordinary payload support still fails.
            if let Some(body_pos) = retracting_body
                && b.support_pos(*p) == Some(body_pos)
                && b.support_offset == view.block(body_pos)?.facing.map(Facing::offset)
            {
                continue;
            }
            return Err(unsupported(format!(
                "movement would change component support at {p:?}; destruction/replacement is outside the retained payload subset"
            )));
        }
    }
    Ok(())
}

fn begin(
    view: RuntimeView<'_>,
    pos: Pos,
    action: PistonBlockEvent,
    requested_facing: Facing,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let body = view.block(pos)?;
    if body.kind != BlockKind::Piston {
        return Ok(RuntimeOutcome::default());
    }
    let mut facts = control_facts(view, pos, &body)?;
    facts.event = action;
    if !builtin_laws().control(facts).execute {
        // Java restores the extended property when canceling a retract.
        return if action != PistonBlockEvent::Extend {
            Ok(RuntimeOutcome {
                delta: Some(delta(
                    view,
                    [(pos, state(body, true))],
                    Vec::new(),
                    DeltaCause::NeighborUpdate,
                )?),
                ..RuntimeOutcome::default()
            })
        } else {
            Ok(RuntimeOutcome::default())
        };
    }
    if action == PistonBlockEvent::Extend {
        let Some(moves) = push_moves(view, pos, &body)? else {
            return Ok(RuntimeOutcome::default());
        };
        start_extension(view, pos, body, moves)
    } else {
        let front = along(
            pos,
            facing(&body)?,
            builtin_laws()
                .geometry(true, false, view.time().section, 0)
                .expect("fixed facts")
                .head,
        )?;
        let callbacks = view
            .carrier(front)
            .map(|c| call(front, PistonEvent::ForceFinish { carrier: c.id }))
            .into_iter()
            .collect();
        Ok(RuntimeOutcome {
            callbacks,
            continuation: Some(PistonEvent::RetractBody {
                body: Box::new(body),
                event: action,
                restored_facing: requested_facing,
            }),
            ..RuntimeOutcome::default()
        })
    }
}

fn start_extension(
    view: RuntimeView<'_>,
    pos: Pos,
    body: Block,
    moves: Vec<BlockMove>,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let dir = facing(&body)?;
    let effects = builtin_laws()
        .geometry(false, false, view.time().section, 0)
        .expect("fixed facts");
    let front = along(pos, dir, effects.head)?;
    if !effects.head_carrier || effects.body_carrier {
        return Err(unsupported("unsupported extension carrier effects"));
    }
    let changes: Vec<_> = moves
        .iter()
        .flat_map(|m| [m.from, m.to])
        .chain([front])
        .collect();
    no_attached_components(view, &changes, None)?;
    let mut plan = MotionPlan::new(
        DeltaCause::PistonExtend { piston: pos },
        Some(PistonEvent::ExtendBody {
            body: Box::new(body.clone()),
        }),
    );
    for movement in &moves {
        plan.write(
            movement.to,
            moving(movement.block.clone(), dir, true, false),
            true,
        );
    }
    plan.write(front, moving(head(&body)?, dir, true, true), true);
    // In a linear push every old source is overwritten by the next carrier
    // or by the source head. No premature air write is made at these cells.
    let mut jobs = VecDeque::new();
    for source in moves.iter().map(|m| m.from).chain([front]) {
        jobs.extend(adjacent_jobs(view, source, false, false)?);
    }
    plan.notify(jobs);
    Ok(RuntimeOutcome {
        continuation: Some(plan.event()),
        ..Default::default()
    })
}

fn retract_body(
    view: RuntimeView<'_>,
    pos: Pos,
    body: &Block,
    action: PistonBlockEvent,
    restored_facing: Facing,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let effects = builtin_laws()
        .geometry(true, false, view.time().section, 0)
        .expect("fixed facts");
    if !effects.body_carrier || effects.head_carrier {
        return Err(unsupported("unsupported retraction carrier effects"));
    }
    no_attached_components(view, &[pos], Some(pos))?;
    // The event's data argument restores the facing of the carried body;
    // movement itself uses the facing of the current receiving state.
    let mut restored = state(body.clone(), false);
    restored.facing = Some(restored_facing);
    if restored.observed_name.is_some() {
        restored
            .observed_properties
            .insert("facing".into(), facing_name(restored_facing).into());
    }
    let after = moving(restored, facing(body)?, false, true);
    let mut plan = MotionPlan::new(
        DeltaCause::PistonRetract { piston: pos },
        Some(PistonEvent::RetractPayload {
            body: Box::new(body.clone()),
            event: action,
        }),
    );
    // Flags 276 suppress the write's shape pass. Entity registration precedes
    // the explicit ordinary and shape notification batches for the body.
    plan.write(pos, after, false);
    plan.notify(adjacent_jobs(view, pos, false, true)?);
    Ok(RuntimeOutcome {
        continuation: Some(plan.event()),
        ..Default::default()
    })
}

fn retract_payload(
    view: RuntimeView<'_>,
    pos: Pos,
    body: &Block,
    action: PistonBlockEvent,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let dir = facing(body)?;
    let effects = builtin_laws()
        .geometry(true, false, view.time().section, 0)
        .expect("fixed facts");
    let source = along(pos, dir, effects.payload)?;
    let front = along(pos, dir, effects.head)?;
    let block = view.block(source)?;
    let matching = block
        .piston_entity
        .as_deref()
        .is_some_and(|e| e.extending && e.facing == dir);
    // The body is a carrier now, so this stage supplies the captured body
    // facts instead of querying electrical input on a non-piston block.
    let mut facts = crate::piston_motion_law::ControlFacts {
        event: action,
        sticky: crate::piston_variant(body) == crate::PistonVariant::Sticky,
        matching_carrier: matching,
        ..Default::default()
    };
    if facts.sticky
        && !matching
        && action == PistonBlockEvent::Retract
        && block.kind != BlockKind::Air
    {
        facts.pullable = movable(source, &block)?;
    }
    let decision = builtin_laws().control(facts);
    if decision.finish_payload {
        let carrier = view
            .carrier(source)
            .ok_or(RuntimeError::CarrierConflict(source))?
            .id;
        return Ok(RuntimeOutcome {
            callbacks: vec![call(source, PistonEvent::ForceFinish { carrier })],
            ..RuntimeOutcome::default()
        });
    }
    if decision.pull {
        no_attached_components(view, &[front, source], None)?;
        let mut plan = MotionPlan::new(DeltaCause::PistonRetract { piston: pos }, None);
        // move(false) removes the old head with flags 276 before the payload
        // destination write (324), then clears the vacated source (82).
        if view.block(front)?.kind == BlockKind::PistonHead {
            plan.write(front, Block::new(BlockKind::Air), false);
        }
        plan.write(front, moving(block, dir, false, false), true);
        plan.write(source, Block::new(BlockKind::Air), false);
        let mut jobs = shape_jobs(view, source)?;
        jobs.extend(adjacent_jobs(view, source, false, false)?);
        plan.notify(jobs);
        return Ok(RuntimeOutcome {
            continuation: Some(plan.event()),
            ..Default::default()
        });
    }
    if decision.remove_head {
        let before = view.block(front)?;
        if before.kind != BlockKind::Air {
            if view.carrier(front).is_some() {
                return Err(unsupported("head carrier remained after forced completion"));
            }
            return Ok(RuntimeOutcome {
                delta: Some(delta(
                    view,
                    [(front, Block::new(BlockKind::Air))],
                    Vec::new(),
                    DeltaCause::PistonRetract { piston: pos },
                )?),
                callbacks: vec![notification(view, pos, &[front], false)?],
                ..RuntimeOutcome::default()
            });
        }
    }
    Ok(RuntimeOutcome::default())
}

fn finish_or_advance(
    view: RuntimeView<'_>,
    pos: Pos,
    forced: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let block = view.block(pos)?;
    let entity = block
        .piston_entity
        .as_deref()
        .ok_or(RuntimeError::CarrierConflict(pos))?;
    let carrier = *view
        .carrier(pos)
        .ok_or(RuntimeError::CarrierConflict(pos))?;
    let step = builtin_laws().carrier(
        carrier.history,
        forced,
        entity.source,
        view.time().game_tick,
    );
    if step.complete {
        let mut after = if step.discard {
            Block::new(BlockKind::Air)
        } else {
            (*entity.pushed_block).clone()
        };
        if after.kind == BlockKind::PistonHead && !head_supported(view, pos, &after)? {
            after = Block::new(BlockKind::Air);
        }
        if crate::device_program::program(&after).is_some_and(|p| p.definition.preprocess_shapes) {
            // postProcessState evaluates all six neighbor directions before
            // the arrival write. Its facing-side observer callback can enqueue
            // a destination tick even though that cell is still moving.
            let queued = super::devices::preprocess(
                view,
                pos,
                &after,
                along(
                    pos,
                    after
                        .facing
                        .ok_or_else(|| unsupported("device output required"))?,
                    -1,
                )?,
            )?;
            return Ok(RuntimeOutcome {
                delta: Some(delta(
                    view,
                    [(pos, after.clone())],
                    vec![],
                    DeltaCause::NeighborUpdate,
                )?),
                carriers: vec![CarrierEffect::Retire {
                    position: pos,
                    expected: carrier,
                }],
                queued,
                outputs: vec![],
                callbacks: vec![super::devices::event(
                    pos,
                    crate::device_program::Callback::Added,
                    None,
                )],
                continuation: Some(PistonEvent::DeviceAfterArrival {
                    block: Box::new(after),
                }),
            });
        }
        let mut jobs = adjacent_jobs(view, pos, false, true)?;
        let own = NeighborJob {
            target: pos,
            source: pos,
            shape: false,
        };
        if after.kind == BlockKind::Piston {
            jobs.push_front(own.clone());
        } // onBlockAdded
        jobs.push_back(own); // the explicit post-completion updateNeighbor
        let callbacks = vec![call(pos, PistonEvent::Notify { jobs })];
        return Ok(RuntimeOutcome {
            delta: Some(delta(
                view,
                [(pos, after)],
                Vec::new(),
                DeltaCause::NeighborUpdate,
            )?),
            carriers: vec![CarrierEffect::Retire {
                position: pos,
                expected: carrier,
            }],
            callbacks,
            ..RuntimeOutcome::default()
        });
    }
    let delay = builtin_laws()
        .geometry(false, false, view.time().section, 0)
        .expect("fixed facts")
        .next_tick_delay;
    let queued = if step.again {
        vec![QueueRequest::CarrierTick {
            game_tick: next_tick(view.time().game_tick, delay)?,
            carrier: carrier.id,
            call: call(pos, PistonEvent::CarrierTick),
        }]
    } else {
        Vec::new()
    };
    Ok(RuntimeOutcome {
        carriers: vec![CarrierEffect::Update {
            position: pos,
            expected: carrier,
            history: step.history,
        }],
        queued,
        ..RuntimeOutcome::default()
    })
}
