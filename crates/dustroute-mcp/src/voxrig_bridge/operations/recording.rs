use super::*;
use crate::bridge::{
    BlockUpdateEvent, ObservedBlockState, RecordingClock, UpdateRecording, UpdateRecordingStarted,
};
use dustroute_ir::{EventCause, EventKind, EventSource, TransitionPhase};
impl VoxrigBridge {
    pub async fn start_update_recording(
        &self,
        min: Pos,
        max: Pos,
        dimension: &str,
        max_events: usize,
    ) -> Result<UpdateRecordingStarted, BotBridgeError> {
        self.require_dimension(dimension).await?;
        let started = self
            .operations()?
            .start_block_recording(
                Region {
                    min: array(min),
                    max: array(max),
                },
                max_events,
            )
            .await
            .map_err(native_error)?;
        if started.dimension != dimension {
            self.operations()?
                .stop_block_recording(&started.recording_id)
                .await
                .map_err(native_error)?;
            return Err(fail("dimension changed while starting recording"));
        }
        Ok(UpdateRecordingStarted {
            recording_id: started.recording_id,
            started_game_tick: started.client_tick,
            clock: RecordingClock::ClientFrame20Hz,
        })
    }
    pub async fn stop_update_recording(
        &self,
        id: &str,
        dimension: &str,
    ) -> Result<UpdateRecording, BotBridgeError> {
        let record = self
            .operations()?
            .stop_block_recording(id)
            .await
            .map_err(native_error)?;
        if record.issue.is_some() || record.started.dimension != dimension {
            return Err(fail(format!(
                "native recording was invalidated: {:?}",
                record.issue
            )));
        }
        self.require_dimension(dimension).await?;
        let state = |s: NativeBlockState| ObservedBlockState {
            name: s.name,
            properties: s.properties,
        };
        let mut last_tick = None;
        let mut order = 0;
        let events = record
            .events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                if last_tick != Some(event.client_tick) {
                    last_tick = Some(event.client_tick);
                    order = 0;
                }
                let sub_tick_order = order;
                order += 1;
                BlockUpdateEvent {
                    sequence: index as u64 + 1,
                    game_tick: event.client_tick,
                    sub_tick_order,
                    phase: TransitionPhase::Unknown,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::PacketObservation,
                    source: EventSource::LiveVoxrig,
                    cause_sequence: None,
                    native_packet: Some(crate::bridge::NativePacketBoundary {
                        connection_id: record.started.connection_id,
                        receive_sequence: event.receive_sequence,
                        packet_order: event.packet_order,
                    }),
                    pos: super::super::pos(event.position),
                    before: Some(state(event.before)),
                    after: Some(state(event.after)),
                }
            })
            .collect();
        Ok(UpdateRecording {
            recording_id: record.started.recording_id,
            started_game_tick: record.started.client_tick,
            stopped_game_tick: record.stopped_client_tick,
            clock: RecordingClock::ClientFrame20Hz,
            seen_events: record.seen_events,
            truncated: record.truncated,
            events,
        })
    }
}
