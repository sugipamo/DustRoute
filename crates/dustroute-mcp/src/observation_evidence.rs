//! Fresh observations and their explicitly different evidence sources. Archived
//! records remain readable facts; they cannot construct a fresh capability.
#[cfg(feature = "voxrig")]
use crate::bridge::BotBridgeError;
use crate::bridge::{ServerReadback, ValidatedRegion};
use dustroute_physical::Pos;
use dustroute_translate::snapshot::MinecraftSnapshot;
use serde::{Deserialize, Serialize};

/// Durable evidence identifies its own schema and source. Existing server
/// receipts retain their exact serialization; client records have no server tick.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ObservationEvidence {
    ServerConfirmed(ServerReadback),
    ClientReconstructed(ClientReadback),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientReadback {
    pub schema_version: ClientReadbackSchema,
    pub kind: ClientReadbackKind,
    pub connection_id: u64,
    pub dimension: String,
    pub min: Pos,
    pub max: Pos,
    pub checked_cells: usize,
    pub receive_sequence: u64,
    pub client_tick: u64,
    pub received_revision: u64,
    pub client_revision: u64,
    pub captured_at_millis: u64,
    pub moving: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientReadbackSchema {
    #[serde(rename = "dustroute.client-readback.v1")]
    V1,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientReadbackKind {
    ClientReconstructed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ObservationRecord {
    #[serde(flatten)]
    pub snapshot: MinecraftSnapshot,
    pub readback: ObservationEvidence,
}

/// Constructed only from a validated live transport result. No Deserialize.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct FreshRegion(ObservationRecord);
impl FreshRegion {
    pub(crate) fn server(region: ValidatedRegion) -> Self {
        let record = region.into_record();
        Self(ObservationRecord {
            snapshot: record.snapshot,
            readback: ObservationEvidence::ServerConfirmed(record.readback),
        })
    }
    #[cfg(feature = "voxrig")]
    pub(crate) fn client(
        region: crate::voxrig_bridge::ClientRegion,
    ) -> Result<Self, BotBridgeError> {
        let view = region.observation();
        Ok(Self(ObservationRecord {
            snapshot: region.snapshot().clone(),
            readback: ObservationEvidence::ClientReconstructed(ClientReadback {
                schema_version: ClientReadbackSchema::V1,
                kind: ClientReadbackKind::ClientReconstructed,
                connection_id: view.received.connection_id,
                dimension: view.dimension.clone(),
                min: region.snapshot().min,
                max: region.snapshot().max,
                checked_cells: view.blocks.len(),
                receive_sequence: view.received.receive_sequence.ok_or_else(|| {
                    BotBridgeError::Protocol("client receive boundary unavailable".into())
                })?,
                client_tick: view.client_tick,
                received_revision: view.received.revision,
                client_revision: view.client_revision,
                captured_at_millis: u64::try_from(view.received.captured_at.as_millis()).map_err(
                    |_| BotBridgeError::Protocol("client observation age overflow".into()),
                )?,
                moving: region.is_moving(),
            }),
        }))
    }
    pub(crate) fn into_record(self) -> ObservationRecord {
        self.0
    }
    pub(crate) fn into_stationary_record(self) -> Result<ObservationRecord, String> {
        if self.0.readback.moving()
            || self
                .0
                .snapshot
                .blocks
                .iter()
                .any(|b| b.name == "minecraft:moving_piston")
        {
            return Err("moving observation cannot authorize a construction step".into());
        }
        Ok(self.0)
    }
}

#[derive(Debug)]
pub(crate) struct ObservationInterval {
    pub server_ticks: Option<u64>,
    pub client_ticks: Option<u64>,
}
impl ObservationEvidence {
    pub(crate) fn interval_since(&self, first: &Self) -> Result<ObservationInterval, String> {
        let backwards = || "observation clock moved backwards or connection changed".to_string();
        match (first, self) {
            (Self::ServerConfirmed(a), Self::ServerConfirmed(b)) => Ok(ObservationInterval {
                server_ticks: Some(
                    b.start_game_tick
                        .checked_sub(a.end_game_tick)
                        .ok_or_else(backwards)?,
                ),
                client_ticks: None,
            }),
            (Self::ClientReconstructed(a), Self::ClientReconstructed(b)) => {
                if a.connection_id != b.connection_id
                    || a.dimension != b.dimension
                    || b.receive_sequence < a.receive_sequence
                    || b.captured_at_millis < a.captured_at_millis
                    || b.received_revision < a.received_revision
                    || b.client_revision < a.client_revision
                {
                    return Err(backwards());
                }
                Ok(ObservationInterval {
                    server_ticks: None,
                    client_ticks: Some(
                        b.client_tick
                            .checked_sub(a.client_tick)
                            .ok_or_else(backwards)?,
                    ),
                })
            }
            _ => Err("observation source changed between samples".into()),
        }
    }
    pub(crate) fn moving(&self) -> bool {
        matches!(self,Self::ClientReconstructed(readback) if readback.moving)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn client() -> ObservationEvidence {
        ObservationEvidence::ClientReconstructed(ClientReadback {
            schema_version: ClientReadbackSchema::V1,
            kind: ClientReadbackKind::ClientReconstructed,
            connection_id: 1,
            dimension: "minecraft:overworld".into(),
            min: Pos::new(0, 80, 0),
            max: Pos::new(0, 80, 0),
            checked_cells: 1,
            receive_sequence: 100,
            client_tick: 10,
            received_revision: 5,
            client_revision: 0,
            captured_at_millis: 500,
            moving: false,
        })
    }
    #[test]
    fn stationary_block_name_does_not_hide_an_active_client_carrier() {
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 80, 0),
            max: Pos::new(0, 80, 0),
            blocks: vec![dustroute_translate::snapshot::MinecraftSnapshotBlock {
                pos: Pos::new(0, 80, 0),
                name: "minecraft:piston".into(),
                properties: [("extended".into(), "true".into())].into(),
            }],
        };
        let mut moving = client();
        let ObservationEvidence::ClientReconstructed(ref mut r) = moving else {
            unreachable!()
        };
        r.moving = true;
        assert!(
            FreshRegion(ObservationRecord {
                snapshot: snapshot.clone(),
                readback: moving,
            })
            .into_stationary_record()
            .is_err()
        );
        assert!(
            FreshRegion(ObservationRecord {
                snapshot,
                readback: client(),
            })
            .into_stationary_record()
            .is_ok()
        );
    }
    #[test]
    fn client_evidence_has_no_server_tick_and_rejects_restart_or_reversed_boundaries() {
        let first = client();
        let mut second = client();
        let ObservationEvidence::ClientReconstructed(ref mut r) = second else {
            unreachable!()
        };
        r.client_tick = 30;
        r.captured_at_millis = 1500;
        let interval = second.interval_since(&first).unwrap();
        assert_eq!(interval.client_ticks, Some(20));
        assert!(interval.server_ticks.is_none());
        for case in 0..5 {
            let mut bad = second.clone();
            let ObservationEvidence::ClientReconstructed(ref mut r) = bad else {
                unreachable!()
            };
            match case {
                0 => r.connection_id += 1,
                1 => r.client_tick = 0,
                2 => r.receive_sequence = 1,
                3 => r.received_revision = 1,
                _ => r.captured_at_millis = 0,
            }
            assert!(bad.interval_since(&first).is_err());
        }
        let value = serde_json::to_value(first).unwrap();
        assert_eq!(value["kind"], "client_reconstructed");
        assert!(serde_json::from_value::<ServerReadback>(value).is_err());
    }
}
