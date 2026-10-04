//! A job's archived intentions and freshly observed diagnostics stay distinct.
use super::{ElectricalEditPreview, StateSummary};
use crate::assembly_registry::TargetServer;
use crate::construction_jobs::{JobAttempt, JobRecord, JobState};
use crate::operations::mutation::Success;
use crate::placement_source::PlacementSource;
use dustroute_library::world_edit::WorldEditScope;
use dustroute_translate::piston_construction::ElectricalWorkRegion;
use dustroute_translate::snapshot::MinecraftSnapshotBlock;
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use uuid::Uuid;

fn automatic_expansion(record: &JobRecord) -> bool {
    record.before.blocks.len()
        + record.after.blocks.len()
        + record
            .boundaries
            .iter()
            .map(|b| b.changes.len())
            .sum::<usize>()
        <= 512
}
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum JobDetails {
    Full(Box<JobRecord>),
    Summary(Box<CompactJob>),
}
impl JobDetails {
    pub fn new(record: &JobRecord, include_intention: bool) -> Self {
        if include_intention || automatic_expansion(record) {
            Self::Full(Box::new(record.clone()))
        } else {
            Self::Summary(Box::new(CompactJob {
                schema: record.schema.clone(),
                execution_profile: record.execution_profile.clone(),
                id: record.id,
                player: record.player.clone(),
                source_revision_id: record.source_revision_id,
                source: record.source.clone(),
                target: record.target.clone(),
                scope: record.scope.clone(),
                regions: record.regions.clone(),
                completed_regions: record.completed_regions,
                state: record.state,
                forward_cancelled: record.forward_cancelled,
                active_operation_id: record.active_operation_id,
                attempts: record.attempts.clone(),
                before: StateSummary::new(&record.before),
                after: StateSummary::new(&record.after),
                boundaries: record
                    .boundaries
                    .iter()
                    .map(|b| BoundarySummary {
                        changed_positions: b.changes.len(),
                        changes: b.changes.iter().take(64).cloned().collect(),
                        truncated: b.changes.len() > 64,
                    })
                    .collect(),
            }))
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CompactJob {
    schema: String,
    execution_profile: String,
    id: Uuid,
    player: String,
    source_revision_id: Uuid,
    source: PlacementSource,
    target: TargetServer,
    scope: WorldEditScope,
    regions: Vec<ElectricalWorkRegion>,
    completed_regions: usize,
    state: JobState,
    forward_cancelled: bool,
    active_operation_id: Option<Uuid>,
    attempts: Vec<JobAttempt>,
    before: StateSummary,
    after: StateSummary,
    boundaries: Vec<BoundarySummary>,
}
#[derive(Clone, Debug, Serialize)]
struct BoundarySummary {
    changed_positions: usize,
    changes: Vec<MinecraftSnapshotBlock>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct JobSummary {
    schema_version: &'static str,
    ok: Success,
    job: JobDetails,
    executable_plan_restored: bool,
    future_regions_verified: bool,
    functional_behavior_verified: bool,
    intention_expanded: bool,
    history_scope: &'static str,
    next_step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_full_intention: Option<&'static str>,
}
impl JobSummary {
    pub fn new(record: &JobRecord, include_intention: bool) -> Self {
        let expanded = automatic_expansion(record);
        Self {
            schema_version: "dustroute.construction-job-response.v2", ok: Success,
            job: JobDetails::new(record, include_intention), executable_plan_restored: false,
            future_regions_verified: false, functional_behavior_verified: false,
            intention_expanded: expanded || include_intention,
            history_scope: "durable intention and verified batch progress; not live-world evidence",
            next_step: "observe or plan_next/plan_undo; show and confirm each fresh operation",
            read_full_intention: (!expanded).then_some("manage_construction_job(action=get, include_intention=true); historical intention, not executable proof"),
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct JobStagePreview {
    #[serde(flatten)]
    preview: ElectricalEditPreview,
    job_id: Uuid,
    job: JobDetails,
    future_regions_verified: bool,
}
impl JobStagePreview {
    pub fn new(preview: ElectricalEditPreview, record: &JobRecord) -> Self {
        Self {
            preview,
            job_id: record.id,
            job: JobDetails::new(record, false),
            future_regions_verified: false,
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct JobObservation {
    ok: Success,
    job_id: Uuid,
    state: JobState,
    matches_verified_prefix: bool,
    difference: Option<String>,
    observation: ObservedJob,
    write_authorized: bool,
    history_advanced: bool,
    interpretation: &'static str,
    hidden_runtime_observed: bool,
}
#[derive(Debug, Serialize)]
struct ObservedJob {
    observation_id: crate::snapshot_content::ObservationId,
    readback: crate::observation_evidence::ObservationEvidence,
    bounds: RegionBounds,
    differences: Vec<dustroute_translate::diagnostic::difference::SnapshotDifference>,
    differing_positions: usize,
    differences_truncated: bool,
}
impl JobObservation {
    pub fn new(
        record: &JobRecord,
        bounds: RegionBounds,
        matches: Result<(), String>,
        observation: crate::observation_evidence::SharedObservationRecord,
        differences: Vec<dustroute_translate::diagnostic::difference::SnapshotDifference>,
    ) -> Self {
        let count = differences.len();
        Self {
            ok: Success,
            job_id: record.id,
            state: record.state,
            matches_verified_prefix: matches.is_ok(),
            difference: matches.err(),
            observation: ObservedJob {
                observation_id: observation.observation_id,
                readback: observation.readback,
                bounds,
                differences: differences.into_iter().take(64).collect(),
                differing_positions: count,
                differences_truncated: count > 64,
            },
            write_authorized: false,
            history_advanced: false,
            interpretation: "literal differences; cause and ownership are not inferred",
            hidden_runtime_observed: false,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum JobResponse {
    Summary(Box<JobSummary>),
    Stage(Box<JobStagePreview>),
    Observed(Box<JobObservation>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_physical::{Pos, Region};
    use dustroute_translate::piston_construction::ElectricalBoundary;
    use dustroute_translate::snapshot::MinecraftSnapshot;

    #[test]
    fn history_expansion_keeps_complete_intention_separate_from_execution_authority() {
        let bounds = Region::new(Pos::new(0, 0, 0), Pos::new(512, 0, 0));
        let blocks = (0..512)
            .map(|x| MinecraftSnapshotBlock {
                pos: Pos::new(x, 0, 0),
                name: "minecraft:stone".into(),
                properties: Default::default(),
            })
            .collect::<Vec<_>>();
        let mut record = JobRecord {
            schema: crate::construction_jobs::SCHEMA.into(),
            execution_profile: "recorded-profile".into(),
            id: Uuid::from_u128(1),
            player: "Tester".into(),
            source_revision_id: Uuid::from_u128(2),
            source: PlacementSource::CircuitRevision {
                revision_id: Uuid::from_u128(2),
            },
            target: TargetServer {
                host: "test-target".into(),
                port: 25565,
                version: "1.21.11".into(),
                dimension: "minecraft:overworld".into(),
                enabled_features: vec![],
            },
            before: MinecraftSnapshot {
                min: bounds.min,
                max: bounds.max,
                blocks,
            },
            after: MinecraftSnapshot {
                min: bounds.min,
                max: bounds.max,
                blocks: vec![],
            },
            scope: WorldEditScope::entire(bounds),
            regions: vec![],
            boundaries: vec![],
            completed_regions: 0,
            state: JobState::Cancelled,
            forward_cancelled: true,
            active_operation_id: Some(Uuid::from_u128(3)),
            attempts: vec![],
        };
        let small = serde_json::to_value(JobSummary::new(&record, false)).unwrap();
        assert_eq!(small["intention_expanded"], true);
        assert!(small.get("read_full_intention").is_none());
        assert_eq!(small["job"], serde_json::to_value(&record).unwrap());
        record.boundaries.push(ElectricalBoundary {
            changes: (0..65)
                .map(|x| MinecraftSnapshotBlock {
                    pos: Pos::new(x, 0, 0),
                    name: "minecraft:air".into(),
                    properties: Default::default(),
                })
                .collect(),
        });
        record.completed_regions = 1;
        let compact = serde_json::to_value(JobSummary::new(&record, false)).unwrap();
        assert_eq!(compact["intention_expanded"], false);
        assert_eq!(compact["job"]["before"]["non_air_blocks"], 512);
        assert!(compact["job"]["before"].get("blocks").is_none());
        assert_eq!(compact["job"]["boundaries"][0]["changed_positions"], 65);
        assert_eq!(
            compact["job"]["boundaries"][0]["changes"]
                .as_array()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(compact["job"]["boundaries"][0]["truncated"], true);
        let complete = serde_json::to_value(JobSummary::new(&record, true)).unwrap();
        assert_eq!(complete["job"], serde_json::to_value(&record).unwrap());
        assert_eq!(complete["intention_expanded"], true);
        assert_eq!(
            complete["read_full_intention"],
            compact["read_full_intention"]
        );
        for view in [&compact, &complete] {
            assert_eq!(view["job"]["state"], "cancelled");
            assert_eq!(view["job"]["forward_cancelled"], true);
            for flag in [
                "executable_plan_restored",
                "future_regions_verified",
                "functional_behavior_verified",
            ] {
                assert_eq!(view[flag], false);
            }
        }
        assert_eq!(record.boundaries[0].changes.len(), 65);
        assert_eq!(record.active_operation_id, Some(Uuid::from_u128(3)));
    }
}
