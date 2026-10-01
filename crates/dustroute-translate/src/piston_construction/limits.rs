//! Model and work-partition budgets. These do not set live transport or policy limits.
pub const MAX_CONTEXT_BLOCKS: usize = 4096;
pub const MAX_STAGE_CHANGES: usize = 64;
pub const MAX_JOB_CHANGES: usize = 4096;
// The dependency algorithm uses one u64 per node; raising this requires a new representation.
pub const MAX_WORK_STAGES: usize = u64::BITS as usize;
pub const MAX_STEP_WAIT_TICKS: u64 = 1200;
pub const MAX_BATCH_WRITES: usize = 32;
