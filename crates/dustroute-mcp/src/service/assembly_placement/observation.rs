//! Fresh observation outcomes. Only complete, unchanged samples expose a
//! baseline; saved records are history, never a live capability.
use super::*;
use crate::observation_evidence::{FreshRegion, ObservationEvidence};
use dustroute_translate::{snapshot::MinecraftSnapshot, world_reverse::RegionBounds};

const SAMPLE_INTERVAL: u16 = 20;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum SampleClock {
    Client,
}

#[derive(Debug, serde::Serialize)]
struct SampleEvidence {
    readbacks: [ObservationEvidence; 2],
    sample_interval_ticks: u16,
    sample_interval_clock: SampleClock,
    observed_server_tick_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_client_tick_interval: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
struct StableSamples {
    snapshot: MinecraftSnapshot,
    #[serde(flatten)]
    evidence: SampleEvidence,
    matching_samples: usize,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ObservationOutcome {
    Matches {
        reason: Option<String>,
        #[serde(flatten)]
        samples: StableSamples,
    },
    Changed {
        reason: String,
        #[serde(flatten)]
        samples: StableSamples,
    },
    HistoryUnavailable {
        reason: &'static str,
        samples: [MinecraftSnapshot; 2],
        #[serde(flatten)]
        evidence: SampleEvidence,
    },
    TargetMismatch {
        reason: String,
    },
    ObservationIncomplete {
        reason: String,
        cause: FailureCause,
    },
}

/// Deliberately no Deserialize: archived reports cannot be used as fresh reads.
#[derive(Debug, serde::Serialize)]
pub(super) struct InstanceObservation {
    #[serde(flatten)]
    outcome: ObservationOutcome,
    observed_at_unix_ms: Option<u64>,
    runtime_history_reconstructed: bool,
}
impl InstanceObservation {
    pub(super) fn recorded(&self) -> crate::recorded_instance::RecordedInstanceObservation {
        use crate::recorded_instance::{
            RecordedInstanceObservation, RecordedObservationOutcome as Recorded,
        };
        let InstanceObservation {
            outcome,
            observed_at_unix_ms,
            runtime_history_reconstructed,
        } = self;
        let outcome = match outcome {
            ObservationOutcome::Matches { reason, samples } => Recorded::Matches {
                reason: reason.clone(),
                samples: record_samples(samples),
            },
            ObservationOutcome::Changed { reason, samples } => Recorded::Changed {
                reason: reason.clone(),
                samples: record_samples(samples),
            },
            ObservationOutcome::HistoryUnavailable {
                reason,
                samples,
                evidence,
            } => Recorded::HistoryUnavailable {
                reason: (*reason).into(),
                samples: samples.clone(),
                evidence: record_evidence(evidence),
            },
            ObservationOutcome::TargetMismatch { reason } => Recorded::TargetMismatch {
                reason: reason.clone(),
            },
            ObservationOutcome::ObservationIncomplete { reason, cause } => {
                Recorded::ObservationIncomplete {
                    reason: reason.clone(),
                    cause: cause.clone(),
                }
            }
        };
        RecordedInstanceObservation {
            outcome,
            observed_at_unix_ms: *observed_at_unix_ms,
            runtime_history_reconstructed: *runtime_history_reconstructed,
        }
    }
    pub(super) fn matches_reference(&self) -> bool {
        matches!(self.outcome, ObservationOutcome::Matches { .. })
    }
    pub(super) fn refusal(&self) -> Option<FailureCause> {
        match &self.outcome {
            ObservationOutcome::Matches { .. } => None,
            ObservationOutcome::ObservationIncomplete { cause, .. } => Some(cause.clone()),
            ObservationOutcome::TargetMismatch { reason } => {
                Some(FailureCause::new(CauseKind::InvalidState, reason))
            }
            ObservationOutcome::Changed { reason, .. } => {
                Some(FailureCause::new(CauseKind::VerificationMismatch, reason))
            }
            ObservationOutcome::HistoryUnavailable { reason, .. } => Some(FailureCause::new(
                CauseKind::ObservationUnavailable,
                *reason,
            )),
        }
    }
}

fn record_samples(samples: &StableSamples) -> crate::recorded_instance::RecordedStableSamples {
    let StableSamples {
        snapshot,
        evidence,
        matching_samples,
    } = samples;
    crate::recorded_instance::RecordedStableSamples {
        snapshot: snapshot.clone(),
        evidence: record_evidence(evidence),
        matching_samples: *matching_samples,
    }
}
fn record_evidence(evidence: &SampleEvidence) -> crate::recorded_instance::RecordedSampleEvidence {
    let SampleEvidence {
        readbacks,
        sample_interval_ticks,
        sample_interval_clock,
        observed_server_tick_interval,
        observed_client_tick_interval,
    } = evidence;
    crate::recorded_instance::RecordedSampleEvidence {
        readbacks: readbacks.clone(),
        sample_interval_ticks: *sample_interval_ticks,
        sample_interval_clock: match sample_interval_clock {
            SampleClock::Client => crate::recorded_instance::SampleClock::Client,
        },
        observed_server_tick_interval: *observed_server_tick_interval,
        observed_client_tick_interval: *observed_client_tick_interval,
    }
}
impl std::fmt::Display for InstanceObservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (status, reason) = match &self.outcome {
            ObservationOutcome::Matches { reason, .. } => ("matches", reason.as_deref()),
            ObservationOutcome::Changed { reason, .. } => ("changed", Some(reason.as_str())),
            ObservationOutcome::HistoryUnavailable { reason, .. } => {
                ("history unavailable", Some(*reason))
            }
            ObservationOutcome::TargetMismatch { reason } => {
                ("target mismatch", Some(reason.as_str()))
            }
            ObservationOutcome::ObservationIncomplete { reason, .. } => {
                ("observation incomplete", Some(reason.as_str()))
            }
        };
        write!(f, "instance observation: {status}")?;
        if let Some(reason) = reason {
            write!(f, "; {reason}")?;
        }
        Ok(())
    }
}

pub(super) fn stable_baseline(
    observation: &InstanceObservation,
) -> Result<MinecraftSnapshot, FailureCause> {
    match &observation.outcome {
        ObservationOutcome::Matches { samples, .. }
        | ObservationOutcome::Changed { samples, .. } => Ok(samples.snapshot.clone()),
        ObservationOutcome::ObservationIncomplete { cause, .. } => Err(cause.clone()),
        _ => Err(FailureCause::new(
            CauseKind::ObservationUnavailable,
            "operation needs complete, unchanged samples",
        )),
    }
}

fn compare_samples(
    first: FreshRegion,
    second: FreshRegion,
    expected: &MinecraftSnapshot,
    version: &str,
    bounds: RegionBounds,
) -> Result<ObservationOutcome, String> {
    let first = first.into_record();
    let second = second.into_record();
    let interval = second.readback.interval_since(&first.readback)?;
    for sample in [&first.snapshot, &second.snapshot] {
        if sample.min != bounds.min || sample.max != bounds.max {
            return Err("exact complete observation region required".into());
        }
        crate::revision::blocks(sample)?;
    }
    let native_moving = first.readback.moving() || second.readback.moving();
    let evidence = SampleEvidence {
        readbacks: [first.readback, second.readback],
        sample_interval_ticks: SAMPLE_INTERVAL,
        sample_interval_clock: SampleClock::Client,
        observed_server_tick_interval: interval.server_ticks,
        observed_client_tick_interval: interval.client_ticks,
    };
    let moving = [&first.snapshot, &second.snapshot]
        .iter()
        .any(|s| s.blocks.iter().any(|b| b.name == "minecraft:moving_piston"));
    if moving
        || native_moving
        || ValidatedAssemblyPlacement::matches(&second.snapshot, &first.snapshot, version).is_err()
    {
        return Ok(ObservationOutcome::HistoryUnavailable {
            reason: "moving or changing observation; exact pending callbacks and motion history cannot be recovered from snapshots",
            samples: [first.snapshot, second.snapshot],
            evidence,
        });
    }
    let comparison = ValidatedAssemblyPlacement::matches(&second.snapshot, expected, version);
    let samples = StableSamples {
        snapshot: second.snapshot,
        evidence,
        matching_samples: 2,
    };
    Ok(match comparison {
        Ok(()) => ObservationOutcome::Matches {
            reason: None,
            samples,
        },
        Err(reason) => ObservationOutcome::Changed { reason, samples },
    })
}

impl AssemblyService<'_> {
    pub(super) async fn observe_instance(&self, record: &PlacedAssembly) -> InstanceObservation {
        let bounds = RegionBounds::new(record.expected.min, record.expected.max);
        let observed: Result<ObservationOutcome, FailureCause> = async {
            self.policy
                .authorize_dimension(&record.target.dimension)
                .map_err(FailureCause::from)?;
            self.policy
                .validate_region(bounds)
                .map_err(FailureCause::from)?;
            let status = self.bridge.status().await.map_err(FailureCause::from)?;
            if let Err(reason) = record
                .target
                .check(&status)
                .and_then(|()| server_contract(&status, &record.target.dimension))
            {
                return Ok(ObservationOutcome::TargetMismatch { reason });
            }
            let first = self
                .bridge
                .scan_region_fresh(bounds.min, bounds.max, &record.target.dimension)
                .await
                .map_err(FailureCause::from)?;
            // Unchanged samples do not establish empty queues or hidden history.
            self.bridge
                .wait_ticks(SAMPLE_INTERVAL, &record.target.dimension)
                .await
                .map_err(FailureCause::from)?;
            let status = self.bridge.status().await.map_err(FailureCause::from)?;
            record.target.check(&status)?;
            let second = self
                .bridge
                .scan_region_fresh(bounds.min, bounds.max, &record.target.dimension)
                .await
                .map_err(FailureCause::from)?;
            let expected = if record.state == InstanceState::Removed {
                MinecraftSnapshot {
                    min: bounds.min,
                    max: bounds.max,
                    blocks: vec![],
                }
            } else {
                record.expected.clone()
            };
            compare_samples(first, second, &expected, &status.version, bounds)
                .map_err(|e| FailureCause::new(CauseKind::ObservationIncomplete, e))
        }
        .await;
        InstanceObservation {
            outcome: observed.unwrap_or_else(|cause| ObservationOutcome::ObservationIncomplete {
                reason: cause.message.clone(),
                cause,
            }),
            observed_at_unix_ms: now_ms().ok(),
            runtime_history_reconstructed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(occupied: bool) -> MinecraftSnapshot {
        serde_json::from_value(json!({
            "min":{"x":0,"y":0,"z":0}, "max":{"x":1,"y":0,"z":0},
            "blocks":if occupied {vec![json!({"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone"})]} else {vec![]}
        })).unwrap()
    }
    // Transport validation has its own tests. These synthetic receipts exercise
    // the subsequent sample classification, not server confirmation itself.
    fn sample(snapshot: MinecraftSnapshot, tick: u64) -> FreshRegion {
        let record = crate::bridge::ConfirmedRegion {
            readback: serde_json::from_value(json!({
                "schema_version":"dustroute.server-readback.v1", "kind":"server_confirmed",
                "request_id":tick.to_string(), "dimension":"minecraft:overworld",
                "min":snapshot.min,"max":snapshot.max,"checked_cells":(snapshot.max.x-snapshot.min.x+1) as u64,
                "start_game_tick":tick,"end_game_tick":tick,"nonce":"0".repeat(32),
                "snapshot_sha256":"0".repeat(64),"corrections":[],"hidden_runtime_observed":false
            }))
            .unwrap(),
            snapshot,
        };
        FreshRegion::server(
            crate::bridge::ValidatedRegion::test_sample(record),
            &crate::snapshot_content::SnapshotContents::default(),
        )
        .unwrap()
    }
    fn observation(outcome: ObservationOutcome) -> InstanceObservation {
        InstanceObservation {
            outcome,
            observed_at_unix_ms: Some(123),
            runtime_history_reconstructed: false,
        }
    }

    #[test]
    fn unchanged_samples_keep_match_and_difference_separate_without_claiming_history() {
        let actual = snapshot(true);
        let bounds = RegionBounds::new(actual.min, actual.max);
        for expected in [actual.clone(), snapshot(false)] {
            let first = sample(actual.clone(), 10);
            let second = sample(actual.clone(), 32);
            let readbacks = json!([
                serde_json::to_value(&first).unwrap()["readback"],
                serde_json::to_value(&second).unwrap()["readback"]
            ]);
            let reason = ValidatedAssemblyPlacement::matches(&actual, &expected, "1.21.11").err();
            let result =
                observation(compare_samples(first, second, &expected, "1.21.11", bounds).unwrap());
            assert_eq!(result.matches_reference(), reason.is_none());
            assert_eq!(stable_baseline(&result).unwrap(), actual);
            let record = result.recorded();
            let bytes = dustroute_codec::storage::encode("instance.observation.v1", &record, 65536)
                .unwrap();
            let reopened: crate::recorded_instance::RecordedInstanceObservation =
                dustroute_codec::storage::decode("instance.observation.v1", &bytes, 65536).unwrap();
            assert_eq!(
                serde_json::to_value(&reopened).unwrap(),
                serde_json::to_value(&result).unwrap()
            );
            let report = crate::recorded_instance::RecordedInstanceReport {
                observation: record,
                revalidation: crate::recorded_instance::RecordedRevalidation::Failed {
                    reason: "target context was not revalidated".into(),
                },
                removal_eligible: false,
                diagnosis: None,
            };
            let bytes =
                dustroute_codec::storage::encode("instance.report.v1", &report, 65536).unwrap();
            let reopened: crate::recorded_instance::RecordedInstanceReport =
                dustroute_codec::storage::decode("instance.report.v1", &bytes, 65536).unwrap();
            let public = serde_json::to_value(reopened).unwrap();
            assert_eq!(public["revalidation"]["status"], "failed");
            assert_eq!(public["removal_eligible"], false);
            assert!(public.get("diagnosis").is_none());
            // Pin the existing public shape, including null reason and the
            // distinction between client wait and measured server interval.
            assert_eq!(
                serde_json::to_value(&result).unwrap(),
                json!({
                    "status":if reason.is_none() {"matches"} else {"changed"},
                    "reason":reason,"snapshot":actual,"readbacks":readbacks,
                    "sample_interval_ticks":20,"sample_interval_clock":"client",
                    "observed_server_tick_interval":22,"matching_samples":2,
                    "observed_at_unix_ms":123,"runtime_history_reconstructed":false
                })
            );
        }
    }

    #[test]
    fn changing_or_moving_samples_never_expose_an_operation_baseline() {
        let empty = snapshot(false);
        let bounds = RegionBounds::new(empty.min, empty.max);
        let changed = compare_samples(
            sample(empty.clone(), 10),
            sample(snapshot(true), 30),
            &empty,
            "1.21.11",
            bounds,
        )
        .unwrap();
        let mut moving = snapshot(true);
        moving.blocks[0].name = "minecraft:moving_piston".into();
        moving.blocks[0].properties = [
            ("facing".into(), "east".into()),
            ("type".into(), "normal".into()),
        ]
        .into();
        let moving = compare_samples(
            sample(moving.clone(), 10),
            sample(moving, 30),
            &empty,
            "1.21.11",
            bounds,
        )
        .unwrap();
        for outcome in [
            changed,
            moving,
            ObservationOutcome::TargetMismatch {
                reason: "target changed".into(),
            },
            ObservationOutcome::ObservationIncomplete {
                reason: "missing receipt".into(),
                cause: FailureCause::new(CauseKind::ObservationIncomplete, "missing receipt"),
            },
        ] {
            let result = observation(outcome);
            assert!(!result.matches_reference());
            assert!(result.refusal().is_some());
            assert!(!result.refusal().unwrap().message.contains("\"blocks\""));
            assert!(stable_baseline(&result).is_err());
            let record = result.recorded();
            let bytes = dustroute_codec::storage::encode("instance.observation.v1", &record, 65536)
                .unwrap();
            let reopened: crate::recorded_instance::RecordedInstanceObservation =
                dustroute_codec::storage::decode("instance.observation.v1", &bytes, 65536).unwrap();
            assert_eq!(
                serde_json::to_value(&reopened).unwrap(),
                serde_json::to_value(&result).unwrap()
            );
            let saved = serde_json::to_value(result).unwrap();
            assert!(saved.get("snapshot").is_none());
            assert_eq!(saved["runtime_history_reconstructed"], false);
        }
    }

    #[test]
    fn reversed_clock_and_partial_regions_remain_observation_errors() {
        let empty = snapshot(false);
        let bounds = RegionBounds::new(empty.min, empty.max);
        assert!(
            compare_samples(
                sample(empty.clone(), 30),
                sample(empty.clone(), 10),
                &empty,
                "1.21.11",
                bounds
            )
            .is_err()
        );
        let mut partial = empty.clone();
        partial.max.x = 0;
        assert!(
            compare_samples(
                sample(empty.clone(), 10),
                sample(partial, 30),
                &empty,
                "1.21.11",
                bounds
            )
            .is_err()
        );
    }

    #[test]
    fn live_observation_cannot_deserialize_or_restore_from_a_record() {
        // Inference becomes ambiguous, failing compilation, if either inverse
        // trait is added. No extra compile-test dependency is required.
        trait AmbiguousIfRestorable<A> {
            fn marker() {}
        }
        impl<T> AmbiguousIfRestorable<()> for T {}
        impl<T: serde::Deserialize<'static>> AmbiguousIfRestorable<u8> for T {}
        impl<T: From<crate::recorded_instance::RecordedInstanceObservation>>
            AmbiguousIfRestorable<u16> for T
        {
        }
        let _ = <InstanceObservation as AmbiguousIfRestorable<_>>::marker;
    }
}
