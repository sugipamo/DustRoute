//! Request-local live facts, separate from saved results and replay authority.
use crate::failure::ExecutionProgress;
use crate::performance::Phase;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ActivityAction {
    InvokeOperation,
    UndoOperation,
    AnalyzeRegion,
}

#[derive(Clone, Debug)]
pub(crate) struct Activity(Arc<Mutex<State>>);
#[derive(Debug)]
struct State {
    started: Instant,
    finished: Option<Instant>,
    action: ActivityAction,
    next_span: u64,
    spans: Vec<(u64, Phase, Instant)>,
    last_phase: Option<Phase>,
    progress: Option<ExecutionProgress>,
}
#[derive(Serialize)]
pub(crate) struct ActivitySnapshot {
    schema: &'static str,
    pub active: bool,
    action: ActivityAction,
    elapsed_ms: u128,
    phase: Option<Phase>,
    phase_active: bool,
    phase_elapsed_ms: Option<u128>,
    /// Actual independently verified execution facts; absent during admission.
    execution_progress: Option<ExecutionProgress>,
    /// Mutations cannot be stopped safely by cancelling this observer.
    cancellable: bool,
}
impl Activity {
    pub fn new(action: ActivityAction) -> Self {
        Self(Arc::new(Mutex::new(State {
            started: Instant::now(),
            finished: None,
            action,
            next_span: 0,
            spans: Vec::new(),
            last_phase: None,
            progress: None,
        })))
    }
    pub fn snapshot(&self) -> ActivitySnapshot {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let phase = state
            .finished
            .is_none()
            .then(|| state.spans.last())
            .flatten();
        ActivitySnapshot {
            schema: "dustroute.operation_activity.v1",
            active: state.finished.is_none(),
            action: state.action,
            elapsed_ms: state
                .finished
                .unwrap_or_else(Instant::now)
                .duration_since(state.started)
                .as_millis(),
            phase: phase.map(|(_, p, _)| *p).or(state.last_phase),
            phase_active: phase.is_some(),
            phase_elapsed_ms: phase.map(|(_, _, t)| t.elapsed().as_millis()),
            execution_progress: state.progress.clone(),
            cancellable: state.action == ActivityAction::AnalyzeRegion,
        }
    }
    pub fn enter(&self, phase: Phase) -> u64 {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let id = state.next_span;
        state.next_span += 1;
        state.spans.push((id, phase, Instant::now()));
        state.last_phase = Some(phase);
        id
    }
    pub fn leave(&self, id: u64) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .spans
            .retain(|(token, _, _)| *token != id);
    }
    pub fn progress(&self, progress: &ExecutionProgress) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).progress = Some(progress.clone());
    }
    pub fn finish(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.finished.get_or_insert_with(Instant::now);
    }
}
/// Even a dropped request future leaves an inactive observer, not a false success.
pub(crate) struct ActivityGuard(pub Activity);
impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.0.finish();
    }
}
