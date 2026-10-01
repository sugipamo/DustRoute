//! Synthetic execution overhead, not a Minecraft physics or live workload proof.
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{Block, BlockKind, Pos, Region, World};
use std::time::Instant;
#[derive(Clone, Debug, Eq, PartialEq)]
enum Task {
    Seed(&'static str, usize),
    Noop,
}
struct Adapter;
impl RuntimeAdapter for Adapter {
    type Payload = Task;
    const REVISION: &'static str = "measurement.metadata.v1";
    fn validate_initial(_: RuntimeView<'_>) -> Result<(), RuntimeError> {
        Ok(())
    }
    fn handle(
        event: &Invocation<Task>,
        view: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
        let mut out = RuntimeOutcome::default();
        if let Task::Seed(kind, count) = event.call.payload {
            for x in 0..count {
                let position = Pos::new(x as i32, 0, 0);
                if kind == "outputs" {
                    out.outputs.push(OutputEffect {
                        position,
                        block: BlockIdentity::of(&view.block(position)?),
                        value: 1,
                    });
                }
                if kind == "histories" {
                    out.histories.push(HistoryEffect::Record {
                        rule: HistoryRule {
                            key: "measurement".into(),
                            policy: HistoryPolicy {
                                window: 128,
                                threshold: 64,
                            },
                        },
                        position,
                    });
                }
            }
        }
        Ok(out)
    }
}
fn main() {
    for kind in ["outputs", "histories", "queue"] {
        for count in [0, 64, 256, 1024, 4096] {
            let mut world = World::new();
            for x in 0..4096 {
                world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            }
            let mut rt = SynchronousWorldRuntime::<Adapter>::new(
                world,
                Region::new(Pos::default(), Pos::new(4095, 0, 0)),
                Default::default(),
            )
            .unwrap();
            rt.input_now(RuntimeCall {
                target: Pos::default(),
                payload: Task::Seed(kind, count),
            })
            .unwrap();
            rt.run_until_idle().unwrap();
            if kind == "queue" {
                for x in 0..count.max(128) {
                    rt.enqueue(QueueRequest::External {
                        game_tick: 1,
                        call: RuntimeCall {
                            target: Pos::new(x as i32, 0, 0),
                            payload: Task::Noop,
                        },
                    })
                    .unwrap();
                }
            }
            let start = Instant::now();
            for _ in 0..128 {
                if kind != "queue" {
                    rt.input_now(RuntimeCall {
                        target: Pos::default(),
                        payload: Task::Noop,
                    })
                    .unwrap();
                }
                let record = rt.microstep().unwrap().unwrap();
                assert!(
                    record.delta.is_none()
                        && record.output_changes.is_empty()
                        && record.history_changes.is_empty()
                );
            }
            println!(
                "{}",
                serde_json::json!({"schema":"dustroute.runtime-metadata-scaling.v1","debug_assertions":cfg!(debug_assertions),"kind":kind,"metadata_entries":count,"queued_at_start":if kind=="queue" {count.max(128)}else{0},"world_blocks":4096,"measured_events":128,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"setup_included":false})
            );
        }
    }
}
