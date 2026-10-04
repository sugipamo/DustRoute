//! Application dispatch keeps typed results until a public MCP handler encodes them.
use super::*;
use crate::blueprint_mcp::{Command, Response};

#[derive(Debug)]
pub(super) enum BlueprintCommandResponse {
    Blueprint(Box<Response>),
    PermissionDenied(FailureCause),
}
impl Serialize for BlueprintCommandResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Blueprint(response) => response.serialize(serializer),
            Self::PermissionDenied(cause) => cause.as_response().serialize(serializer),
        }
    }
}
impl BlueprintCommandResponse {
    fn failure(message: impl ToString) -> Self {
        Self::Blueprint(Box::new(Response::failure(message)))
    }
}

impl DustRouteMcp {
    pub(super) async fn blueprint_command(
        &self,
        command: Command,
        player_override: Option<&str>,
        required: bool,
    ) -> Option<BlueprintCommandResponse> {
        let player = match self.resolve_player(player_override) {
            Ok(player) => player,
            Err(error) => return required.then(|| BlueprintCommandResponse::failure(error)),
        };
        if let Err(error) = self.player_scope().authorize(&player) {
            return Some(BlueprintCommandResponse::PermissionDenied(
                error.at(FailurePhase::Admission),
            ));
        }
        let store = self.state_store.clone();
        match tokio::task::spawn_blocking(move || {
            crate::blueprint_mcp::execute(&store, &player, command)
        })
        .await
        {
            Ok(Ok(Some(result))) => Some(BlueprintCommandResponse::Blueprint(Box::new(result))),
            Ok(Ok(None)) => required
                .then(|| BlueprintCommandResponse::failure("unknown Blueprint operation ID")),
            Ok(Err(error)) => Some(BlueprintCommandResponse::failure(error)),
            Err(error) => Some(BlueprintCommandResponse::failure(error)),
        }
    }
}

/// Live activity augments a historical result without becoming stored proof.
#[derive(Serialize)]
pub(super) struct ResponseWithActivity<T: Serialize> {
    #[serde(flatten)]
    pub response: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<crate::operations::ActivitySnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint_mcp::BlueprintRead;

    #[tokio::test]
    async fn blueprint_authorization_refuses_before_creating_a_store() {
        let root = super::super::test_support::temporary();
        let policy = McpPolicy {
            allowed_players: BTreeSet::from(["other".into()]),
            ..Default::default()
        };
        let mut service = DustRouteMcp::with_policy_and_player(policy, "builder");
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let response = service
            .blueprint_command(Command::Read(BlueprintRead::Archive), None, true)
            .await
            .unwrap();
        let expected = super::super::test_support::decode_reply(&legacy_cause_reply(
            FailureCause::from(service.policy.authorize_player("builder").unwrap_err())
                .at(FailurePhase::Admission),
        ))
        .unwrap();
        match &response {
            BlueprintCommandResponse::PermissionDenied(error) => {
                assert_eq!(error.kind, CauseKind::PermissionDenied);
                assert_eq!(error.phase, Some(FailurePhase::Admission));
            }
            _ => panic!("authorization must stop before dispatch"),
        }
        let public: Value =
            super::super::test_support::decode_reply(&typed_reply(response)).unwrap();
        assert_eq!(public, expected);
        assert_eq!(public["ok"], false);
        assert_eq!(public["error_code"], "permission_denied");
        assert_eq!(public["failure"]["progress"], Value::Null);
        assert_eq!(public["recovery"]["same_operation_replay_allowed"], false);
        assert!(!root.exists());
    }
}
