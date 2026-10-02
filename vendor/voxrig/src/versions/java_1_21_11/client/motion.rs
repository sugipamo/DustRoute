//! Position provenance shared by native controls and stationary admission.
use serde::Serialize;

/// Last actual own-player position/correction packet, after native relative decoding.
#[derive(Clone, Debug, Serialize)]
pub struct ReceivedPose {
    /// Login/respawn/configuration generation.
    pub generation: u64,
    /// Position-packet receive ordinal, not a tick or acknowledgement of a move.
    pub receive_sequence: u64,
    /// Resolved feet position.
    pub position: [f64; 3],
    /// Resolved yaw/pitch.
    pub rotation: [f32; 2],
    /// Resolved packet velocity if its relative baseline was available.
    pub velocity: Option<[f64; 3]>,
}
/// A locally submitted position. Not a physics prediction or server receipt.
#[derive(Clone, Debug, Serialize)]
pub struct PositionSubmission {
    /// Monotonic local attempt ID, never sent as a native movement sequence.
    pub attempt_id: u64,
    /// World generation at submission.
    pub generation: u64,
    /// Receive boundary before I/O.
    pub after_receive_sequence: u64,
    /// Requested feet position.
    pub position: [f64; 3],
    /// Requested yaw/pitch.
    pub rotation: [f32; 2],
    /// Complete frame dispatched; does not certify accepted motion.
    pub dispatched: bool,
    /// Later correction/reset which superseded this local attempt.
    pub superseded_at: Option<u64>,
}
/// What the current position represents. No inference from silence or elapsed time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PositionBasis {
    /// No supported current provenance.
    #[default]
    Unavailable,
    /// Current position came from an own-player correction/position packet.
    Received,
    /// A movement send is unresolved, including cancelled writer acquisition.
    PendingSubmission,
    /// Complete local position frame sent, without an own-position echo.
    Submitted,
}
/// Current position basis and retained history; serialized data grants no authority.
#[derive(Clone, Debug, Default, Serialize)]
pub struct OwnMotion {
    /// Provenance for the current position.
    pub position_basis: PositionBasis,
    /// Last actual position receipt, retained across local submissions/reset.
    pub received_pose: Option<ReceivedPose>,
    /// Last local send attempt, including cancellation.
    pub last_submission: Option<PositionSubmission>,
    /// Latest invalidation reason; cleared by a supported position receipt.
    pub invalidation: Option<String>,
    #[serde(skip)]
    next_attempt: u64,
}
impl OwnMotion {
    pub(super) fn receive(&mut self, pose: ReceivedPose) {
        if let Some(attempt) = &mut self.last_submission {
            attempt.superseded_at.get_or_insert(pose.receive_sequence);
        }
        self.received_pose = Some(pose);
        self.position_basis = PositionBasis::Received;
        self.invalidation = None;
    }
    pub(super) fn invalidate(&mut self, sequence: u64, reason: &str) {
        if let Some(attempt) = &mut self.last_submission {
            attempt.superseded_at.get_or_insert(sequence);
        }
        self.position_basis = PositionBasis::Unavailable;
        self.invalidation = Some(reason.into());
    }
    pub(super) fn begin(
        &mut self,
        generation: u64,
        sequence: u64,
        position: [f64; 3],
        rotation: [f32; 2],
    ) -> anyhow::Result<()> {
        self.next_attempt = self
            .next_attempt
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("position attempt IDs exhausted"))?;
        self.last_submission = Some(PositionSubmission {
            attempt_id: self.next_attempt,
            generation,
            after_receive_sequence: sequence,
            position,
            rotation,
            dispatched: false,
            superseded_at: None,
        });
        self.position_basis = PositionBasis::PendingSubmission;
        Ok(())
    }
    pub(super) fn dispatched(&mut self) {
        self.last_submission
            .as_mut()
            .expect("before-I/O attempt")
            .dispatched = true;
        self.position_basis = PositionBasis::Submitted;
    }
    pub(super) fn received_position(&self, generation: u64, position: Option<[f64; 3]>) -> bool {
        self.position_basis == PositionBasis::Received
            && self
                .received_pose
                .as_ref()
                .is_some_and(|p| p.generation == generation && Some(p.position) == position)
    }
}
