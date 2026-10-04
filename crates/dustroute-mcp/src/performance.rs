//! Opt-in, request-local measurements. Never changes budgets or world behavior.
//! Phase durations are inclusive: nested phases must not be added together.
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
/// Request-local measurement category. It stays typed inside the workflow;
/// serde supplies protocol spelling only at an explicit output boundary.
pub enum Phase {
    Normalization,
    Analysis,
    Verification,
    Gaze,
    Status,
    Scan,
    ContentIntern,
    #[cfg(feature = "voxrig")]
    NativeObserve,
    #[cfg(feature = "voxrig")]
    NativeConvert,
    DiscoveryMerge,
    DiscoveryNeighbors,
    StaticValidation,
    ResponseEncode,
    ModelQueue,
    ModelProof,
    MutationQueue,
    Wait,
    Preview,
    Write,
    Checkpoint,
    StoreRead,
    StoreEncode,
    FileWrite,
    FileSync,
    FileRename,
    DirectorySync,
}

#[derive(Debug, Default, Serialize)]
pub struct PhaseMeasurement {
    pub calls: u64,
    pub elapsed_ms: f64,
    pub cells: u64,
    pub bytes: u64,
    pub requested_ticks: u64,
    pub commands: u64,
    /// Actual cell materialization/conversion, distinct from requested scan volume.
    pub materialized_cells: u64,
    pub cache_hits: u64,
}

#[derive(Serialize)]
pub struct Measurement {
    schema: &'static str,
    pub operation: String,
    pub elapsed_ms: f64,
    pub debug_assertions: bool,
    /// Inclusive wall time, including awaits; not CPU time or server ticks.
    pub phases: BTreeMap<Phase, PhaseMeasurement>,
}

impl std::fmt::Display for Measurement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dustroute.performance.trace.v2 operation={:?} elapsed_ms={:.3} debug_assertions={}",
            self.operation, self.elapsed_ms, self.debug_assertions
        )?;
        for (phase, measurement) in &self.phases {
            write!(f, " {phase:?}={measurement:?}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct Capture {
    metrics: Option<Arc<Mutex<BTreeMap<Phase, PhaseMeasurement>>>>,
    activity: Option<crate::operations::Activity>,
}
tokio::task_local! { static ASYNC_CAPTURE: Capture; }
thread_local! { static SYNC_CAPTURE: RefCell<Capture> = RefCell::new(Capture::default()); }

pub(crate) fn current() -> Capture {
    ASYNC_CAPTURE
        .try_with(Clone::clone)
        .unwrap_or_else(|_| SYNC_CAPTURE.with(|c| c.borrow().clone()))
}

impl Capture {
    /// Explicitly propagate the request into spawn_blocking, without leaking it
    /// into another job that later reuses the worker thread.
    pub fn in_blocking<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Restore(Capture);
        impl Drop for Restore {
            fn drop(&mut self) {
                SYNC_CAPTURE.with(|c| *c.borrow_mut() = self.0.clone());
            }
        }
        let _restore = Restore(SYNC_CAPTURE.with(|c| c.replace(self.clone())));
        work()
    }

    pub fn record_queue(&self, started: Instant) {
        if let Some(capture) = &self.metrics {
            let mut phases = capture.lock().unwrap_or_else(|e| e.into_inner());
            let phase = phases.entry(Phase::ModelQueue).or_default();
            phase.calls += 1;
            phase.elapsed_ms += millis(started.elapsed());
        }
    }
}

/// Attach live reporting while retaining any opt-in performance counters.
pub(crate) async fn with_activity<T>(
    activity: Option<crate::operations::Activity>,
    work: impl Future<Output = T>,
) -> T {
    let mut capture = current();
    capture.activity = activity;
    ASYNC_CAPTURE.scope(capture, work).await
}
pub(crate) fn execution_progress(progress: &crate::failure::ExecutionProgress) {
    if let Some(activity) = current().activity {
        activity.progress(progress);
    }
}

pub(crate) fn tool_progress(response: &rmcp::model::CallToolResponse) {
    if current().activity.is_none() {
        return;
    }
    #[derive(serde::Deserialize)]
    struct FailedFacts {
        progress: Option<crate::failure::ExecutionProgress>,
    }
    #[derive(serde::Deserialize)]
    struct Facts {
        failure: Option<FailedFacts>,
        execution_progress: Option<crate::failure::ExecutionProgress>,
    }
    if let rmcp::model::CallToolResponse::Complete(result) = response {
        for content in &result.content {
            if let rmcp::model::ContentBlock::Text(text) = content
                && let Ok(facts) = serde_json::from_str::<Facts>(&text.text)
                && let Some(progress) = facts
                    .failure
                    .and_then(|f| f.progress)
                    .or(facts.execution_progress)
            {
                execution_progress(&progress);
            }
        }
    }
}

/// Measure library calls without enabling process-wide logging. Independent
/// captures, including concurrent requests, never share counters.
pub async fn measure<T>(operation: &str, work: impl Future<Output = T>) -> (T, Measurement) {
    let capture = Capture {
        metrics: Some(Arc::default()),
        activity: current().activity,
    };
    let started = Instant::now();
    let result = ASYNC_CAPTURE.scope(capture.clone(), work).await;
    let elapsed_ms = millis(started.elapsed());
    let phases = std::mem::take(
        &mut *capture
            .metrics
            .unwrap()
            .lock()
            .unwrap_or_else(|e| e.into_inner()),
    );
    (
        result,
        Measurement {
            schema: "dustroute.performance.v1",
            operation: operation.chars().take(64).collect(),
            elapsed_ms,
            debug_assertions: cfg!(debug_assertions),
            phases,
        },
    )
}

pub(crate) async fn tool<T>(name: &str, work: impl Future<Output = T>) -> T {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !ENABLED.get_or_init(|| std::env::var("DUSTROUTE_PERFORMANCE_TRACE").as_deref() == Ok("1")) {
        return work.await;
    }
    let (result, measurement) = measure(name, work).await;
    // stderr only: tool responses and the stdio MCP stream remain unchanged.
    // No positions, player names, block states or request arguments are logged.
    eprintln!("{measurement}");
    result
}

pub(crate) struct Span {
    capture: Capture,
    phase: Phase,
    started: Option<Instant>,
    activity_span: Option<u64>,
    cells: u64,
    bytes: u64,
    ticks: u64,
    commands: u64,
    materialized_cells: u64,
    cache_hits: u64,
}

pub(crate) fn span(phase: Phase) -> Span {
    let capture = current();
    let activity_span = capture
        .activity
        .as_ref()
        .map(|activity| activity.enter(phase));
    let started = capture.metrics.as_ref().map(|_| Instant::now());
    Span {
        capture,
        phase,
        activity_span,
        started,
        cells: 0,
        bytes: 0,
        ticks: 0,
        commands: 0,
        materialized_cells: 0,
        cache_hits: 0,
    }
}

impl Span {
    pub fn acquisition(mut self, cells: usize, reused: bool) -> Self {
        self.materialized_cells = cells as u64;
        self.cache_hits = u64::from(reused);
        self
    }
    pub fn cells(mut self, min: dustroute_physical::Pos, max: dustroute_physical::Pos) -> Self {
        self.cells = [(min.x, max.x), (min.y, max.y), (min.z, max.z)]
            .into_iter()
            .try_fold(1u64, |v, (lo, hi)| {
                let side = u64::try_from(i64::from(hi) - i64::from(lo) + 1).ok()?;
                v.checked_mul(side)
            })
            .unwrap_or(0);
        self
    }
    pub fn bytes(mut self, bytes: usize) -> Self {
        self.bytes = bytes as u64;
        self
    }
    pub fn ticks(mut self, ticks: u16) -> Self {
        self.ticks = u64::from(ticks);
        self
    }
    pub fn commands(mut self, commands: usize) -> Self {
        self.commands = commands as u64;
        self
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let (Some(activity), Some(id)) = (&self.capture.activity, self.activity_span) {
            activity.leave(id);
        }
        let (Some(started), Some(capture)) = (self.started, &self.capture.metrics) else {
            return;
        };
        let elapsed = started.elapsed();
        let mut phases = capture.lock().unwrap_or_else(|e| e.into_inner());
        let value = phases.entry(self.phase).or_default();
        value.calls += 1;
        value.elapsed_ms += millis(elapsed);
        value.cells += self.cells;
        value.bytes += self.bytes;
        value.requested_ticks += self.ticks;
        value.commands += self.commands;
        value.materialized_cells += self.materialized_cells;
        value.cache_hits += self.cache_hits;
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_are_isolated_and_blocking_workers_restore_their_context() {
        let a = measure("a", async {
            let _span = span(Phase::Wait).ticks(20);
            let capture = current();
            tokio::task::spawn_blocking(move || {
                capture.in_blocking(|| {
                    let _span = span(Phase::ModelProof);
                });
                assert!(
                    current().metrics.is_none(),
                    "the same worker must restore its capture"
                );
            })
            .await
            .unwrap();
        });
        let b = measure("b", async {
            let _span = span(Phase::Status);
        });
        let ((_, a), (_, b)) = tokio::join!(a, b);
        assert_eq!(
            a.phases[&crate::performance::Phase::Wait].requested_ticks,
            20
        );
        assert_eq!(a.phases[&crate::performance::Phase::ModelProof].calls, 1);
        assert!(!a.phases.contains_key(&crate::performance::Phase::Status));
        assert_eq!(b.phases[&crate::performance::Phase::Status].calls, 1);
        assert!(!b.phases.contains_key(&crate::performance::Phase::Wait));
        assert!(current().metrics.is_none());
        assert!(
            tokio::task::spawn_blocking(|| current().metrics.is_none())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn failed_work_keeps_completed_phases_and_disabled_spans_do_nothing() {
        let (_, report) = measure("failure", async {
            let _span = span(Phase::Scan).cells(
                dustroute_physical::Pos::new(-2, -2, -2),
                dustroute_physical::Pos::new(2, 2, 2),
            );
            Err::<(), _>("unloaded")
        })
        .await;
        assert_eq!(report.phases[&crate::performance::Phase::Scan].calls, 1);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].cells, 125);
        assert!(span(Phase::Scan).started.is_none());
    }

    #[test]
    fn blocking_capture_is_restored_when_work_panics() {
        let capture = Capture {
            metrics: Some(Arc::default()),
            activity: current().activity,
        };
        let outcome = std::panic::catch_unwind(|| {
            capture.in_blocking(|| {
                assert!(current().metrics.is_some());
                panic!("simulated local computation failure");
            });
        });
        assert!(outcome.is_err());
        assert!(current().metrics.is_none());
    }
    #[test]
    fn typed_phase_keys_preserve_the_public_mcp_measurement_contract() {
        let phases = [
            (Phase::Normalization, "normalization"),
            (Phase::Analysis, "analysis"),
            (Phase::Verification, "verification"),
            (Phase::Gaze, "gaze"),
            (Phase::Status, "status"),
            (Phase::Scan, "scan"),
            (Phase::ContentIntern, "content_intern"),
            (Phase::DiscoveryMerge, "discovery_merge"),
            (Phase::DiscoveryNeighbors, "discovery_neighbors"),
            (Phase::StaticValidation, "static_validation"),
            (Phase::ResponseEncode, "response_encode"),
            (Phase::ModelQueue, "model_queue"),
            (Phase::ModelProof, "model_proof"),
            (Phase::MutationQueue, "mutation_queue"),
            (Phase::Wait, "wait"),
            (Phase::Preview, "preview"),
            (Phase::Write, "write"),
            (Phase::Checkpoint, "checkpoint"),
            (Phase::StoreRead, "store_read"),
            (Phase::StoreEncode, "store_encode"),
            (Phase::FileWrite, "file_write"),
            (Phase::FileSync, "file_sync"),
            (Phase::FileRename, "file_rename"),
            (Phase::DirectorySync, "directory_sync"),
        ];
        let phases = phases.into_iter();
        #[cfg(feature = "voxrig")]
        let phases = phases.chain([
            (Phase::NativeObserve, "native_observe"),
            (Phase::NativeConvert, "native_convert"),
        ]);
        let phases: Vec<_> = phases.collect();
        let report = Measurement {
            schema: "dustroute.performance.v1",
            operation: "wire-boundary".into(),
            elapsed_ms: 2.5,
            debug_assertions: true,
            phases: phases
                .iter()
                .map(|&(phase, _)| {
                    (
                        phase,
                        PhaseMeasurement {
                            calls: 2,
                            requested_ticks: 20,
                            cells: 125,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
        };
        let wire = serde_json::to_value(report).unwrap();
        assert_eq!(wire["schema"], "dustroute.performance.v1");
        assert_eq!(wire["phases"].as_object().unwrap().len(), phases.len());
        for (_, name) in phases {
            assert_eq!(wire["phases"][name]["calls"], 2);
            assert_eq!(wire["phases"][name]["requested_ticks"], 20);
            assert_eq!(wire["phases"][name]["cells"], 125);
        }
    }
    #[test]
    fn internal_trace_is_non_json_and_escapes_line_breaks_in_the_operation_label() {
        let report = Measurement {
            schema: "dustroute.performance.v1",
            operation: "test\noperation".into(),
            elapsed_ms: 1.5,
            debug_assertions: true,
            phases: BTreeMap::from([(
                Phase::Scan,
                PhaseMeasurement {
                    calls: 1,
                    cells: 8,
                    ..Default::default()
                },
            )]),
        };
        let line = report.to_string();
        assert!(line.starts_with("dustroute.performance.trace.v2 "));
        assert!(!line.contains('\n'));
        assert!(line.contains("Scan="));
        assert!(line.contains("cells: 8"));
    }
}
