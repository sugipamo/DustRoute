//! Invocation and readback are separate transitions. A lost reply cannot make
//! an attempted action eligible for a second automatic invocation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InvocationState {
    Draft,
    Previewed,
    NeedsInspection,
    Verified,
}
impl InvocationState {
    pub fn preview(&mut self) -> Result<(), String> {
        if self.attempted() {
            return Err(
                "operation was already attempted; inspect before creating a new plan".into(),
            );
        }
        *self = Self::Previewed;
        Ok(())
    }
    pub fn is_previewed(self) -> bool {
        self == Self::Previewed
    }
    pub fn attempted(self) -> bool {
        matches!(self, Self::NeedsInspection | Self::Verified)
    }
    pub fn begin(&mut self, require_preview: bool) -> Result<(), String> {
        if self.attempted() || (require_preview && !self.is_previewed()) {
            return Err("operation requires an unused previewed plan".into());
        }
        *self = Self::NeedsInspection;
        Ok(())
    }
    pub fn confirm(&mut self, verified: bool) {
        if self.attempted() {
            *self = if verified {
                Self::Verified
            } else {
                Self::NeedsInspection
            };
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RevisionLifecycle {
    Planned,
    Applying,
    Applied,
    Undoing,
    Undone,
}
impl RevisionLifecycle {
    pub fn can_begin(self, undo: bool) -> bool {
        self == if undo { Self::Applied } else { Self::Planned }
    }
    pub fn begin(&mut self, undo: bool) -> Result<(), String> {
        if !self.can_begin(undo) {
            return Err(
                "revision placement attempt already consumed; inspect current world".into(),
            );
        }
        *self = if undo { Self::Undoing } else { Self::Applying };
        Ok(())
    }
    pub fn confirm(&mut self, undo: bool) {
        *self = if undo { Self::Undone } else { Self::Applied };
    }
}

/// TTL records intentionally reject old Boolean-state archives: a submitted
/// write is not evidence that a repair or rollback completed.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum RepairLifecycle {
    Draft,
    Previewed,
    NeedsInspection,
    Applied,
    Undone,
}
impl RepairLifecycle {
    pub fn preview(&mut self) -> Result<(), String> {
        if !matches!(self, Self::Draft | Self::Previewed) {
            return Err("repair has already been attempted; diagnose and create a new plan".into());
        }
        *self = Self::Previewed;
        Ok(())
    }
    pub fn begin(&mut self, undo: bool, require_preview: bool) -> Result<(), String> {
        let eligible = if undo {
            *self == Self::Applied
        } else {
            *self == Self::Previewed || (!require_preview && *self == Self::Draft)
        };
        if !eligible {
            return Err(
                "repair requires an unused plan; undo requires verified application".into(),
            );
        }
        *self = Self::NeedsInspection;
        Ok(())
    }
    pub fn confirm(&mut self, undo: bool) {
        *self = if undo { Self::Undone } else { Self::Applied };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_invocations_cannot_be_previewed_or_repeated() {
        let mut state = InvocationState::Draft;
        assert!(state.begin(true).is_err());
        state.preview().unwrap();
        state.begin(true).unwrap();
        assert!(state.preview().is_err());
        assert!(state.begin(false).is_err());
        state.confirm(true);
        assert_eq!(state, InvocationState::Verified);
        assert!(state.begin(false).is_err());
    }
    #[test]
    fn repair_receipts_do_not_unlock_undo() {
        let mut state = RepairLifecycle::Draft;
        state.preview().unwrap();
        state.begin(false, true).unwrap();
        let bytes = serde_json::to_string(&state).unwrap();
        let mut restarted: RepairLifecycle = serde_json::from_str(&bytes).unwrap();
        assert!(restarted.begin(false, false).is_err());
        assert!(restarted.begin(true, false).is_err());
        restarted.confirm(false);
        restarted.begin(true, false).unwrap();
        restarted.confirm(true);
        assert!(restarted.begin(false, false).is_err());
    }
}
