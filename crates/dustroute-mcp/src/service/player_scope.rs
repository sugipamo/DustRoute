//! Configured player identity shared by application workflows. It does not
//! capture a player, select a circuit or own transport state.
use crate::McpPolicy;
use crate::failure::{CauseKind, FailureCause};
pub(super) struct PlayerScope<'a> {
    pub configured: Option<&'a str>,
    pub policy: &'a McpPolicy,
}
impl PlayerScope<'_> {
    pub fn resolve(&self, requested: Option<&str>) -> Result<String, FailureCause> {
        match (self.configured, requested) {
            (Some(configured), None) => Ok(configured.to_owned()),
            (Some(configured), Some(requested)) if configured == requested => {
                Ok(requested.to_owned())
            }
            (Some(configured), Some(_)) => Err(FailureCause::new(
                CauseKind::PermissionDenied,
                format!(
                    "player override is not allowed; configured assist player is {configured:?}"
                ),
            )),
            (None, Some(requested)) => Ok(requested.to_owned()),
            (None, None) => Err(FailureCause::new(
                CauseKind::InvalidInput,
                "player is required when DUSTROUTE_ASSIST_PLAYER is not configured",
            )),
        }
    }
    pub fn authorize(&self, player: &str) -> Result<(), FailureCause> {
        self.policy
            .authorize_player(player)
            .map_err(FailureCause::from)
    }
}
