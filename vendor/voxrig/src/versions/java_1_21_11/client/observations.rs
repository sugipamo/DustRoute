//! Bounded generation cache. No freshness, issue or receive metadata is cached.
use super::*;
use crate::versions::java_1_21_11::reconstruction::{ClientBlock, SharedClientRegion};
use std::collections::VecDeque;
use std::time::Duration;

const MAX_REGIONS: usize = 16;
const MAX_CELLS: usize = 65_536;

#[derive(Clone)]
struct Cells {
    region: Region,
    received: Arc<[ObservedBlock]>,
    client: Arc<[ClientBlock]>,
}

#[derive(Default)]
pub(super) struct RegionCache {
    generation: Option<(u64, u64)>,
    entries: VecDeque<Cells>,
    cells: usize,
}
impl RegionCache {
    fn select_generation(&mut self, generation: Option<(u64, u64)>) {
        if generation.is_none() || self.generation != generation {
            self.entries.clear();
            self.cells = 0;
            self.generation = generation;
        }
    }
    fn get(&mut self, region: Region) -> Option<Cells> {
        let index = self.entries.iter().position(|e| e.region == region)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry.clone());
        Some(entry)
    }
    fn insert(&mut self, cells: Cells) {
        let volume = cells.client.len();
        if volume > MAX_CELLS {
            return;
        }
        while self.entries.len() >= MAX_REGIONS || self.cells + volume > MAX_CELLS {
            let Some(old) = self.entries.pop_front() else {
                return;
            };
            self.cells -= old.client.len();
        }
        self.cells += volume;
        self.entries.push_back(cells);
    }
}

impl State {
    pub(super) fn shared_observation(
        &mut self,
        region: Region,
        connection_id: u64,
        captured_at: Duration,
    ) -> Result<SharedClientRegion> {
        if let Some(error) = &self.failure {
            return Err(Error::new(error.kind(), anyhow::anyhow!("{error}")));
        }
        let volume = region.volume()?;
        let (name, dimension) = self
            .world
            .dimension
            .as_ref()
            .context("dimension is not available")?;
        if region.min[1] < dimension.min_y || region.max[1] >= dimension.min_y + dimension.height {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("observation outside dimension"),
            ));
        }
        let name = name.clone();
        // Advancing first is essential: carriers change even without received packets.
        self.reconstruction
            .advance(&self.world, captured_at.as_millis() as u64 / 50);
        let usable =
            self.reconstruction.issue.is_none() && self.reconstruction.recovery_chunks.is_empty();
        self.observations.select_generation(
            usable.then_some((self.world.revision, self.reconstruction.revision)),
        );
        let cached = self.observations.get(region);
        let materialized_cells = if cached.is_some() { 0 } else { volume };
        let cells = if let Some(cells) = cached {
            cells
        } else {
            let mut received = Vec::with_capacity(volume);
            let mut client = Vec::with_capacity(volume);
            for x in region.min[0]..=region.max[0] {
                for y in region.min[1]..=region.max[1] {
                    for z in region.min[2]..=region.max[2] {
                        let position = [x, y, z];
                        let native = self
                            .world
                            .block(position)
                            .map(super::super::native_state)
                            .transpose()?;
                        client.push(
                            self.reconstruction
                                .cell_with_received(position, native.clone()),
                        );
                        received.push(ObservedBlock {
                            position,
                            state: native,
                        });
                    }
                }
            }
            let cells = Cells {
                region,
                received: received.into(),
                client: client.into(),
            };
            if usable && cells.client.iter().all(|b| b.state.is_some()) {
                self.observations.insert(cells.clone());
            }
            cells
        };
        Ok(SharedClientRegion {
            materialized_cells,
            observation: ClientObservation {
                received: Observation {
                    version: MinecraftVersion::Java1_21_11,
                    connection_id,
                    revision: self.world.revision,
                    receive_sequence: Some(self.sequence),
                    captured_at,
                    region,
                    blocks: cells.received,
                },
                dimension: name,
                client_tick: self.reconstruction.tick,
                client_revision: self.reconstruction.revision,
                blocks: cells.client,
                issue: self.reconstruction.issue.clone(),
                recovery_chunks: self
                    .reconstruction
                    .recovery_chunks
                    .iter()
                    .copied()
                    .collect(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_21_11::reconstruction::{MotionProgress, ReconstructionIssue};
    fn state() -> State {
        let mut state = State::default();
        state.world.select_dimension(
            "minecraft:overworld".into(),
            Dimension::new(-64, 384).unwrap(),
        );
        state.world.seed_replay_cell([0, 80, 0], 1);
        state
    }
    fn region() -> Region {
        Region {
            min: [0, 80, 0],
            max: [2, 80, 0],
        }
    }
    #[test]
    fn shared_cells_keep_independent_receive_boundaries_and_owned_wire_compatibility() {
        let mut state = state();
        let first = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        state.sequence += 1; // unrelated packets do not change the world/reconstruction
        let second = state
            .shared_observation(region(), 7, Duration::from_millis(1000))
            .unwrap();
        assert_eq!(first.materialized_cells, 3);
        assert_eq!(second.materialized_cells, 0);
        assert!(Arc::ptr_eq(
            &first.observation.blocks,
            &second.observation.blocks
        ));
        assert!(Arc::ptr_eq(
            &first.observation.received.blocks,
            &second.observation.received.blocks
        ));
        assert_ne!(
            first.observation.received.receive_sequence,
            second.observation.received.receive_sequence
        );
        assert_eq!(
            second.observation.client_tick - first.observation.client_tick,
            20
        );
        assert_eq!(
            serde_json::to_value(&second.observation).unwrap(),
            serde_json::to_value(second.observation.clone().into_owned()).unwrap()
        );
    }
    #[test]
    fn changes_unload_dimension_reset_and_failure_never_reuse_old_cells() {
        let mut state = state();
        let first = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        state.world.seed_replay_cell([0, 80, 0], 0);
        let changed = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        assert_eq!(changed.materialized_cells, 3);
        assert!(!Arc::ptr_eq(
            &first.observation.blocks,
            &changed.observation.blocks
        ));
        assert_eq!(
            changed.observation.blocks[0].state.as_ref().unwrap().name,
            "minecraft:air"
        );
        state.world.unload(&[0; 8]).unwrap();
        let unloaded = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        assert!(
            unloaded
                .observation
                .blocks
                .iter()
                .all(|b| b.state.is_none())
        );
        assert_eq!(
            state
                .shared_observation(region(), 7, Duration::ZERO)
                .unwrap()
                .materialized_cells,
            3
        );
        state.world.reset();
        assert!(
            state
                .shared_observation(region(), 7, Duration::ZERO)
                .is_err()
        );
        state.world.select_dimension(
            "minecraft:the_nether".into(),
            Dimension::new(0, 256).unwrap(),
        );
        state.world.seed_replay_cell([0, 80, 0], 1);
        let dimension = state
            .shared_observation(region(), 8, Duration::ZERO)
            .unwrap();
        assert_eq!(dimension.observation.dimension, "minecraft:the_nether");
        assert!(!Arc::ptr_eq(
            &first.observation.blocks,
            &dimension.observation.blocks
        ));
        state.failure = Some(Error::new(
            ErrorKind::Protocol,
            anyhow::anyhow!("failed receive"),
        ));
        assert!(
            state
                .shared_observation(region(), 8, Duration::ZERO)
                .is_err()
        );
    }
    #[test]
    fn issue_and_recovery_invalidate_cache_even_without_a_revision_change() {
        let mut state = state();
        state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        state.reconstruction.issue = Some(ReconstructionIssue::Limit);
        let issue = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        assert_eq!(issue.materialized_cells, 3);
        assert!(issue.observation.blocks.iter().all(|b| b.state.is_none()));
        state.reconstruction.issue = None;
        state.reconstruction.recovery_chunks.insert([0, 0]);
        assert_eq!(
            state
                .shared_observation(region(), 7, Duration::ZERO)
                .unwrap()
                .materialized_cells,
            3
        );
        state.reconstruction.recovery_chunks.clear();
        assert_eq!(
            state
                .shared_observation(region(), 7, Duration::ZERO)
                .unwrap()
                .materialized_cells,
            3
        );
    }
    #[test]
    fn moving_carriers_advance_before_cache_lookup_without_new_packets() {
        let mut state = state();
        for x in -1..=1 {
            for z in -1..=1 {
                state.world.seed_replay_cell([x * 16, 80, z * 16], 0);
            }
        }
        let body = crate::NativeBlockState {
            name: "minecraft:piston".into(),
            properties: [
                ("facing".into(), "east".into()),
                ("extended".into(), "false".into()),
            ]
            .into(),
        };
        state
            .world
            .seed_replay_cell([0, 80, 0], super::super::super::state_id(&body).unwrap());
        state.world.seed_replay_cell([1, 80, 0], 1);
        state.reconstruction.action(
            &state.world,
            [0, 80, 0],
            Action::Extend,
            Direction::East,
            "minecraft:piston",
            1,
        );
        assert!(
            state.reconstruction.issue.is_none(),
            "{:?}",
            state.reconstruction.issue
        );
        let start = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        let same = state
            .shared_observation(region(), 7, Duration::ZERO)
            .unwrap();
        assert_eq!(same.materialized_cells, 0);
        let next = state
            .shared_observation(region(), 7, Duration::from_millis(50))
            .unwrap();
        assert_eq!(next.materialized_cells, 3);
        assert!(!Arc::ptr_eq(
            &start.observation.blocks,
            &next.observation.blocks
        ));
        assert_eq!(
            next.observation.received.receive_sequence,
            start.observation.received.receive_sequence
        );
        assert_eq!(
            start.observation.blocks[2]
                .moving
                .as_ref()
                .unwrap()
                .progress,
            MotionProgress::Start
        );
        assert_eq!(
            next.observation.blocks[2].moving.as_ref().unwrap().progress,
            MotionProgress::Half
        );
    }
    #[test]
    fn region_cache_is_bounded_and_returns_correct_cells_after_eviction() {
        let mut state = state();
        for x in 0..16 {
            for z in 0..2 {
                state
                    .shared_observation(
                        Region {
                            min: [x, 80, z],
                            max: [x, 80, z],
                        },
                        7,
                        Duration::ZERO,
                    )
                    .unwrap();
            }
        }
        assert_eq!(state.observations.entries.len(), MAX_REGIONS);
        assert!(state.observations.cells <= MAX_CELLS);
        assert_eq!(
            state
                .shared_observation(
                    Region {
                        min: [0, 80, 0],
                        max: [0, 80, 0]
                    },
                    7,
                    Duration::ZERO
                )
                .unwrap()
                .materialized_cells,
            1
        );
        assert!(
            state
                .shared_observation(
                    Region {
                        min: [0, -65, 0],
                        max: [0, -65, 0]
                    },
                    7,
                    Duration::ZERO
                )
                .is_err()
        );
    }
    #[tokio::test]
    async fn concurrent_same_generation_acquisitions_materialize_once() {
        let state = Arc::new(tokio::sync::Mutex::new(state()));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let state = state.clone();
            tasks.push(tokio::spawn(async move {
                state
                    .lock()
                    .await
                    .shared_observation(region(), 7, Duration::ZERO)
                    .unwrap()
            }));
        }
        let mut results = Vec::new();
        for task in tasks {
            results.push(task.await.unwrap());
        }
        assert_eq!(
            results.iter().map(|r| r.materialized_cells).sum::<usize>(),
            3
        );
        assert!(
            results
                .iter()
                .all(|r| Arc::ptr_eq(&r.observation.blocks, &results[0].observation.blocks))
        );
    }
    // The previous production path decoded received cells and reconstructed cells
    // separately. Retained only as an offline benchmark reference, never as a live route.
    fn previous_observation(state: &State, region: Region) -> ClientObservation {
        let mut received = Vec::new();
        let mut blocks = Vec::new();
        for x in region.min[0]..=region.max[0] {
            for y in region.min[1]..=region.max[1] {
                for z in region.min[2]..=region.max[2] {
                    let position = [x, y, z];
                    received.push(ObservedBlock {
                        position,
                        state: state
                            .world
                            .block(position)
                            .map(super::super::super::native_state)
                            .transpose()
                            .unwrap(),
                    });
                    blocks.push(state.reconstruction.cell(&state.world, position));
                }
            }
        }
        ClientObservation {
            received: Observation {
                version: MinecraftVersion::Java1_21_11,
                connection_id: 7,
                revision: state.world.revision,
                receive_sequence: Some(state.sequence),
                captured_at: Duration::ZERO,
                region,
                blocks: received,
            },
            dimension: "minecraft:overworld".into(),
            client_tick: 0,
            client_revision: 0,
            blocks,
            issue: None,
            recovery_chunks: vec![],
        }
    }
    #[test]
    #[ignore = "offline materialization comparison; no Minecraft connection or world writes"]
    fn profile_shared_native_observations() {
        for cells in [720usize, 4096] {
            for sample in 1..=3 {
                let mut state = state();
                let region = if cells == 720 {
                    Region {
                        min: [0, 80, 0],
                        max: [11, 85, 9],
                    }
                } else {
                    Region {
                        min: [0, 80, 0],
                        max: [15, 95, 15],
                    }
                };
                let reference = previous_observation(&state, region);
                let fresh = state.shared_observation(region, 7, Duration::ZERO).unwrap();
                assert_eq!(
                    serde_json::to_value(&fresh.observation).unwrap(),
                    serde_json::to_value(reference).unwrap()
                );
                let started = Instant::now();
                for _ in 0..100 {
                    std::hint::black_box(previous_observation(&state, region));
                }
                let previous_ms = started.elapsed().as_secs_f64() * 1000.0;
                state.observations.select_generation(None);
                let started = Instant::now();
                let mut materialized = 0;
                for _ in 0..100 {
                    let result = state.shared_observation(region, 7, Duration::ZERO).unwrap();
                    materialized += result.materialized_cells;
                    std::hint::black_box(result);
                }
                let shared_ms = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(materialized, cells);
                println!(
                    "NATIVE_SHARING {{\"sample\":{sample},\"cells\":{cells},\"acquisitions\":100,\"previous_materialized_cells\":{},\"shared_materialized_cells\":{materialized},\"previous_ms\":{previous_ms},\"shared_ms\":{shared_ms}}}",
                    cells * 100
                );
            }
        }
    }
}
