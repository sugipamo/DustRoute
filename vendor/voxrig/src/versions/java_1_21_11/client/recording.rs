//! Bounded, ordered received block-state updates. Reconstructed piston frames
//! are deliberately not relabelled as received state packets.
use super::{World, operations::Operations};
use crate::{Error, ErrorKind, NativeBlockState, Region, Result};
use serde::Serialize;

/// Start boundary of a single connection-local recording.
#[derive(Clone, Debug, Serialize)]
pub struct RecordingStarted {
    /// Opaque connection/recording identity.
    pub recording_id: String,
    /// Connection identity.
    pub connection_id: u64,
    /// Native dimension.
    pub dimension: String,
    /// Inclusive complete baseline region.
    pub region: Region,
    /// Receive boundary before the first captured update.
    pub receive_sequence: u64,
    /// Local 20 Hz frame, not a server tick.
    pub client_tick: u64,
}
/// One position written by a received single/multiple block-update packet.
#[derive(Clone, Debug, Serialize)]
pub struct ReceivedBlockUpdate {
    /// Connection packet sequence; multiple cells may share it.
    pub receive_sequence: u64,
    /// Index in the native packet's cell order, including out-of-region cells.
    pub packet_order: usize,
    /// Local 20 Hz frame when the packet was applied.
    pub client_tick: u64,
    /// Changed position.
    pub position: [i32; 3],
    /// Previously received state, not client-side reconstruction.
    pub before: NativeBlockState,
    /// State in the native packet.
    pub after: NativeBlockState,
}
/// A condition that prevents treating the recording as a complete region history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordingIssue {
    /// A region chunk was unloaded or replaced by a bulk snapshot.
    ChunkChanged {
        /// Chunk coordinates.
        chunk: [i32; 2],
    },
    /// Respawn/dimension/reconfiguration invalidated the entity/world baseline.
    WorldChanged,
    /// The connection failed or closed before a successful stop boundary.
    ConnectionUnavailable,
}
/// Captured packet updates. Complete means complete for these packet types,
/// never all block actions, reconstructed frames or server scheduler events.
#[derive(Clone, Debug, Serialize)]
pub struct BlockRecording {
    /// Captured start boundary.
    pub started: RecordingStarted,
    /// Stop receive boundary.
    pub stopped_receive_sequence: u64,
    /// Local 20 Hz stop frame.
    pub stopped_client_tick: u64,
    /// Count of in-region updates, including dropped events after overflow.
    pub seen_events: usize,
    /// True after the configured event limit; never silently treated as complete.
    pub truncated: bool,
    /// Any invalidation since the baseline.
    pub issue: Option<RecordingIssue>,
    /// Received transitions, preserving packet and in-packet order.
    pub events: Vec<ReceivedBlockUpdate>,
}
pub(super) struct Capture {
    started: RecordingStarted,
    baseline: Vec<i32>,
    maximum: usize,
    seen: usize,
    truncated: bool,
    issue: Option<RecordingIssue>,
    events: Vec<ReceivedBlockUpdate>,
}
impl Capture {
    fn new(started: RecordingStarted, world: &World, maximum: usize) -> Result<Self> {
        started.region.volume()?;
        if !(1..=65536).contains(&maximum) {
            return Err(invalid("recording event limit must be 1..65536"));
        }
        let mut baseline = Vec::new();
        let r = started.region;
        for x in r.min[0]..=r.max[0] {
            for y in r.min[1]..=r.max[1] {
                for z in r.min[2]..=r.max[2] {
                    baseline.push(world.block([x, y, z]).ok_or_else(|| {
                        invalid("recording baseline contains unloaded/out-of-dimension cells")
                    })?);
                }
            }
        }
        Ok(Self {
            started,
            baseline,
            maximum,
            seen: 0,
            truncated: false,
            issue: None,
            events: Vec::new(),
        })
    }
    pub fn invalidate(&mut self, issue: RecordingIssue) {
        if self.issue.is_none() {
            self.issue = Some(issue);
        }
    }
    pub fn chunk_changed(&mut self, chunk: [i32; 2]) {
        let r = self.started.region;
        if chunk[0] >= r.min[0].div_euclid(16)
            && chunk[0] <= r.max[0].div_euclid(16)
            && chunk[1] >= r.min[2].div_euclid(16)
            && chunk[1] <= r.max[2].div_euclid(16)
        {
            self.invalidate(RecordingIssue::ChunkChanged { chunk });
        }
    }
    pub fn received(
        &mut self,
        changes: &[([i32; 3], i32)],
        sequence: u64,
        tick: u64,
    ) -> anyhow::Result<()> {
        if self.issue.is_some() {
            return Ok(());
        }
        let r = self.started.region;
        for (packet_order, &(p, id)) in changes.iter().enumerate() {
            if (0..3).any(|i| p[i] < r.min[i] || p[i] > r.max[i]) {
                continue;
            }
            let index = ((p[0] - r.min[0]) as usize * (r.max[1] - r.min[1] + 1) as usize
                + (p[1] - r.min[1]) as usize)
                * (r.max[2] - r.min[2] + 1) as usize
                + (p[2] - r.min[2]) as usize;
            let before = self.baseline[index];
            self.baseline[index] = id;
            self.seen = self.seen.saturating_add(1);
            if self.events.len() >= self.maximum {
                self.truncated = true;
                continue;
            }
            self.events.push(ReceivedBlockUpdate {
                receive_sequence: sequence,
                packet_order,
                client_tick: tick,
                position: p,
                before: super::super::native_state(before)?,
                after: super::super::native_state(id)?,
            });
        }
        Ok(())
    }
    fn finish(self, sequence: u64, tick: u64) -> BlockRecording {
        BlockRecording {
            started: self.started,
            stopped_receive_sequence: sequence,
            stopped_client_tick: tick,
            seen_events: self.seen,
            truncated: self.truncated,
            issue: self.issue,
            events: self.events,
        }
    }
}
fn invalid(message: &str) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}
impl Operations {
    /// Capture received state packets from a fully loaded baseline. One active
    /// recording per connection; piston actions remain in the separate packet trace.
    pub async fn start_block_recording(
        &self,
        region: Region,
        max_events: usize,
    ) -> Result<RecordingStarted> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        if state.recording.is_some() {
            return Err(invalid("block recording already active"));
        }
        let ordinal = state
            .recording_ordinal
            .checked_add(1)
            .ok_or_else(|| invalid("recording ordinal exhausted"))?;
        let started = RecordingStarted {
            recording_id: format!("{}-{ordinal}", self.bot.session.id),
            connection_id: self.bot.session.id,
            dimension: state
                .world
                .dimension
                .as_ref()
                .expect("ready dimension")
                .0
                .clone(),
            region,
            receive_sequence: state.sequence,
            client_tick: self.bot.session.started.elapsed().as_millis() as u64 / 50,
        };
        let capture = Capture::new(started.clone(), &state.world, max_events)?;
        state.recording = Some(capture);
        state.recording_ordinal = ordinal;
        Ok(started)
    }
    /// Stop and return even an invalidated recording; callers must inspect issue
    /// and truncation. A mismatched ID never consumes somebody else's recording.
    pub async fn stop_block_recording(&self, recording_id: &str) -> Result<BlockRecording> {
        let mut state = self.bot.session.state.lock().await;
        if state
            .recording
            .as_ref()
            .is_none_or(|c| c.started.recording_id != recording_id)
        {
            return Err(invalid("block recording missing or ID mismatch"));
        }
        let unavailable = self.bot.session.check(&state).is_err();
        let mut capture = state.recording.take().expect("checked recording");
        if unavailable {
            capture.invalidate(RecordingIssue::ConnectionUnavailable);
        }
        Ok(capture.finish(
            state.sequence,
            self.bot.session.started.elapsed().as_millis() as u64 / 50,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture(max: usize) -> Capture {
        let region = Region {
            min: [0, 80, 0],
            max: [1, 80, 0],
        };
        let mut world = World::default();
        world.select_dimension(
            "minecraft:overworld".into(),
            super::super::super::world::Dimension::new(-64, 384).unwrap(),
        );
        world.seed_replay_cell([0, 80, 0], 0);
        Capture::new(
            RecordingStarted {
                recording_id: "1-1".into(),
                connection_id: 1,
                dimension: "minecraft:overworld".into(),
                region,
                receive_sequence: 5,
                client_tick: 10,
            },
            &world,
            max,
        )
        .unwrap()
    }
    #[test]
    fn keeps_packet_order_and_before_state_with_explicit_overflow() {
        let mut capture = capture(2);
        let stone = super::super::super::state_id(&NativeBlockState {
            name: "minecraft:stone".into(),
            properties: Default::default(),
        })
        .unwrap();
        capture
            .received(
                &[
                    ([8, 80, 0], stone),
                    ([1, 80, 0], stone),
                    ([0, 80, 0], stone),
                ],
                6,
                11,
            )
            .unwrap();
        capture.received(&[([1, 80, 0], 0)], 7, 11).unwrap();
        let recording = capture.finish(8, 13);
        assert_eq!(recording.seen_events, 3);
        assert!(recording.truncated);
        assert_eq!(
            recording
                .events
                .iter()
                .map(|e| e.packet_order)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(recording.events[0].before.name, "minecraft:air");
        assert_eq!(recording.events[0].after.name, "minecraft:stone");
    }
    #[test]
    fn region_chunk_invalidation_never_disappears_after_later_packets() {
        let mut capture = capture(10);
        capture.chunk_changed([1, 1]);
        assert!(capture.issue.is_none());
        capture.chunk_changed([0, 0]);
        capture.received(&[([0, 80, 0], 0)], 6, 11).unwrap();
        let result = capture.finish(7, 12);
        assert_eq!(
            result.issue,
            Some(RecordingIssue::ChunkChanged { chunk: [0, 0] })
        );
        assert!(result.events.is_empty());
    }
}
