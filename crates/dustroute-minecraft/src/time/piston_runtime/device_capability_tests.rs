//! Test-only devices exercise the production sampler, effects and scheduler.
//! These are not Minecraft block implementations or public registry entries.
use super::devices::{prepare_program, step_program};
use super::*;
use crate::device_program::{self, schema::*, *};
use crate::law::static_program::{Expression::*, StaticLaw, Step::*};
use crate::{BlockKind, Facing, WireConnection};
use std::sync::OnceLock;

const P: Pos = Pos::new(0, 4, 0);
const LIT: Property = Property::Bool(BoolProperty::Lit);
const POWERED: Property = Property::Bool(BoolProperty::Powered);
const LAW: StaticLaw = StaticLaw::new(
    &[("lit", 1), ("powered", 1), ("input", 15), ("tick", 1)],
    &[("lit", 1), ("powered", 1), ("level", 15), ("delay", 3)],
    &[
        Set("lit", Input("lit")),
        Set("level", SaturatingSubtract(&Input("input"), &Constant(1))),
        If(
            AtLeast(&Input("input"), &Constant(1)),
            &[
                Set("powered", Constant(1)),
                If(
                    Not(&Input("powered")),
                    &[Set("lit", Not(&Input("lit")))],
                    &[],
                ),
            ],
            &[],
        ),
        If(Not(&Input("tick")), &[Set("delay", Constant(3))], &[]),
    ],
);
const OPERATIONS: &[Operation] = &[
    Operation::Write {
        when: Binding::Constant(1),
        values: &[
            (LIT, Binding::Output("lit")),
            (POWERED, Binding::Output("powered")),
            (Property::Power, Binding::Output("level")),
        ],
        notifications: WriteNotifications::Shapes,
    },
    Operation::Schedule {
        delay: Binding::Output("delay"),
        priority: Binding::Constant(3),
    },
];
const BASE: DeviceSpec = DeviceSpec {
    id: "test.multi-state.off",
    law_id: "test.multi-state",
    law: &LAW,
    properties: &[LIT, POWERED, Property::Power],
    predicates: &[(LIT, 0)],
    physical: crate::physical::PhysicalSpec {
        orientation: Orientation::Output,
        ..BUILTIN_DEVICES[0].spec().physical.spec()
    }
    .checked(),
    signal: Signal::Output,
    signal_level: SignalLevel::Analog,
    initial_neighbor_update: false,
    handlers: &[
        HandlerSpec {
            callback: Callback::Use,
            inputs: &[
                ("lit", Query::State { property: LIT }),
                ("powered", Query::State { property: POWERED }),
                ("input", Query::ReceivingLevel),
                ("tick", Query::Constant { value: 0 }),
            ],
            operations: OPERATIONS,
        },
        HandlerSpec {
            callback: Callback::Tick,
            inputs: &[
                ("lit", Query::State { property: LIT }),
                ("powered", Query::State { property: POWERED }),
                ("input", Query::SideLevel { side: Facing::Up }),
                ("tick", Query::Constant { value: 1 }),
            ],
            operations: OPERATIONS,
        },
    ],
    ..BUILTIN_DEVICES[0].spec()
};
const FIXTURES: [CheckedDevice; 2] = registry([
    BASE.checked(),
    DeviceSpec {
        id: "test.multi-state.on",
        predicates: &[(LIT, 1)],
        ..BASE
    }
    .checked(),
]);
fn programs() -> &'static [DeviceProgram; 2] {
    static PROGRAMS: OnceLock<[DeviceProgram; 2]> = OnceLock::new();
    PROGRAMS.get_or_init(|| FIXTURES.map(|s| s.compile()))
}

struct FixtureAdapter;
impl RuntimeAdapter for FixtureAdapter {
    type Payload = PistonEvent;
    const REVISION: &'static str = "test.typed-device.v1";
    fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
        let b = view.block(P)?;
        device_program::select(programs(), &b)
            .ok_or_else(|| unsupported("fixture variant"))?
            .definition
            .validate_state(&b)
            .map_err(unsupported)
    }
    fn handle(
        inv: &Invocation<PistonEvent>,
        view: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
        let pos = inv.call.target;
        match &inv.call.payload {
            PistonEvent::Device {
                callback,
                source,
                captured,
            } if pos == P => {
                let before = captured.as_deref().cloned().unwrap_or(view.block(pos)?);
                let program = device_program::select(programs(), &before)
                    .ok_or_else(|| unsupported("fixture variant"))?;
                let Some(run) = prepare_program(view, pos, &before, *callback, *source, program)?
                else {
                    return Ok(Default::default());
                };
                step_program(view, pos, run, program)
            }
            PistonEvent::DeviceContinue { run } if pos == P => {
                let program = device_program::select(programs(), &run.before)
                    .ok_or_else(|| unsupported("fixture continuation"))?;
                step_program(view, pos, *run.clone(), program)
            }
            _ => adapter::ElectricalPistonAdapter::handle(inv, view),
        }
    }
}

fn scene(level: u8, observed: bool) -> World {
    let mut world = World::new();
    let b = world.place(BlockKind::RedstoneLamp, P);
    b.powered = Some(false);
    b.power_level = Some(0);
    b.facing = Some(Facing::East);
    b.observed_properties
        .insert("powered".into(), "false".into());
    if observed {
        b.observed_name = Some("minecraft:redstone_lamp".into());
        b.observed_properties.insert("lit".into(), "false".into());
        b.observed_properties.insert("power".into(), "0".into());
    }
    let wire = world.place(BlockKind::RedstoneWire, P.offset(0, 1, 0));
    wire.power_level = Some(level);
    wire.support_offset = Some(Facing::Down.offset());
    wire.wire_connections = Some(
        crate::piston_electrical::HORIZONTAL
            .map(|d| (d, WireConnection::None))
            .into(),
    );
    world
}
fn runtime(
    level: u8,
    observed: bool,
    initial_lit: bool,
) -> SynchronousWorldRuntime<FixtureAdapter> {
    let mut world = scene(level, observed);
    let block = world.get_mut(P).unwrap();
    block.powered = Some(initial_lit);
    if observed {
        block
            .observed_properties
            .insert("lit".into(), initial_lit.to_string());
    }
    let mut rt = SynchronousWorldRuntime::new(
        world,
        Region::new(Pos::new(-5, 0, -5), Pos::new(5, 10, 5)),
        Default::default(),
    )
    .unwrap();
    rt.enqueue(QueueRequest::External {
        game_tick: 1,
        call: super::devices::event(P, Callback::Use, None),
    })
    .unwrap();
    rt
}

#[test]
fn analog_sampling_atomic_multi_state_writes_and_continuations_cover_all_levels() {
    for initial_lit in [false, true] {
        for observed in [false, true] {
            for level in 0..=15u8 {
                let mut rt = runtime(level, observed, initial_lit);
                let mut checkpoints = vec![];
                while rt.microstep().unwrap().is_some() {
                    checkpoints.push(rt.checkpoint());
                }
                let after = rt.view().block(P).unwrap();
                let expected_lit = initial_lit ^ (level > 0);
                assert_eq!(after.powered, Some(expected_lit));
                assert_eq!(
                    after.observed_properties["powered"],
                    (level > 0).to_string()
                );
                assert_eq!(after.power_level, Some(level.saturating_sub(1)));
                let selected = device_program::select(programs(), &after).unwrap();
                assert_eq!(
                    selected.definition.id,
                    if expected_lit {
                        "test.multi-state.on"
                    } else {
                        "test.multi-state.off"
                    }
                );
                for strength in [level.saturating_sub(1), level] {
                    let mut source = after.clone();
                    source.power_level = Some(strength);
                    if observed {
                        source
                            .observed_properties
                            .insert("power".into(), strength.to_string());
                    }
                    for side in crate::piston_electrical::SIDES {
                        let emitted = selected.definition.emission(&source, side).unwrap();
                        let expected = if side == Facing::West { strength } else { 0 };
                        assert_eq!((emitted.weak, emitted.strong), (expected, expected));
                    }
                }
                let writes: Vec<_> = rt
                    .trace()
                    .iter()
                    .flat_map(|r| r.delta.iter().flat_map(|d| &d.changes))
                    .filter(|c| c.position == P)
                    .collect();
                assert!(level == 0 || !writes.is_empty());
                // Every published delta contains the whole transition, not three
                // property writes with callbacks between them.
                for change in writes {
                    assert_eq!(change.after.powered, after.powered);
                    assert_eq!(
                        change.after.observed_properties["powered"],
                        after.observed_properties["powered"]
                    );
                    assert_eq!(change.after.power_level, after.power_level);
                }
                assert!(rt.trace().iter().any(|r| r.invocation.time.game_tick == 4));
                for checkpoint in checkpoints {
                    let mut resumed =
                        SynchronousWorldRuntime::<FixtureAdapter>::from_checkpoint(&checkpoint)
                            .unwrap();
                    resumed.run_until_idle().unwrap();
                    assert_eq!(resumed.state_key(), rt.state_key());
                }
            }
        }
    }
}

#[test]
fn analog_queries_reject_unknown_space_and_inconsistent_state() {
    let world = scene(15, false);
    let region = Region::new(P, P.offset(0, 1, 0));
    let query = crate::piston_electrical::ElectricalWorld::new(&world, region).unwrap();
    assert!(query.receiving_level(P).is_err());
    let mut b = scene(0, true).get(P).unwrap().clone();
    b.power_level = Some(16);
    assert!(programs()[0].definition.validate_state(&b).is_err());
    b.power_level = Some(5);
    assert!(programs()[0].definition.validate_state(&b).is_err());
    b.observed_properties.insert("power".into(), "5".into());
    assert!(programs()[0].definition.validate_state(&b).is_ok());
    b.observed_properties.remove("powered");
    assert!(programs()[0].definition.validate_state(&b).is_err());
}
