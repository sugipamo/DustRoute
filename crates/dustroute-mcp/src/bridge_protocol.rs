//! Mutation transport records. A submission receipt never proves a live state;
//! callers must perform independent readback, including after uncertain errors.
use dustroute_physical::Pos;
use dustroute_translate::native_state::NativeBlockState;
use serde::{Deserialize, Serialize};

pub const MUTATION_PROTOCOL: &str = "dustroute.bridge-mutation.v1";
pub const COMMAND_LIMIT: usize = 32_768;
pub const PHYSICAL_LIMIT: usize = 128;

#[derive(Debug, Deserialize, Serialize)]
pub enum MutationProtocol {
    #[serde(rename = "dustroute.bridge-mutation.v1")]
    V1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandWrite {
    pub pos: Pos,
    pub state: NativeBlockState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhysicalChange {
    Dig {
        pos: Pos,
    },
    Place {
        pos: Pos,
        item: String,
        state: NativeBlockState,
        reference: Pos,
        face: Pos,
    },
}

#[derive(Debug, Serialize)]
pub(crate) struct MutationRequest<'a, T> {
    pub protocol: &'static str,
    pub changes: &'a [T],
    pub dimension: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CommandSubmission {
    pub protocol: MutationProtocol,
    pub submitted_changes: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalPlacementMode {
    MineflayerPlayer,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PhysicalSubmission {
    pub protocol: MutationProtocol,
    pub placed_changes: usize,
    pub placement_mode: PhysicalPlacementMode,
    pub retreat: super::bridge::Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutation_contract_is_shared_with_javascript_and_rejects_retired_receipts() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/mutation-contract.json")).unwrap();
        let command: Vec<CommandWrite> =
            serde_json::from_value(fixture["command"]["changes"].clone()).unwrap();
        let request = MutationRequest {
            protocol: MUTATION_PROTOCOL,
            changes: &command,
            dimension: "minecraft:overworld",
        };
        assert_eq!(serde_json::to_value(request).unwrap(), fixture["command"]);
        let physical: Vec<PhysicalChange> =
            serde_json::from_value(fixture["physical"]["changes"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(MutationRequest {
                protocol: MUTATION_PROTOCOL,
                changes: &physical,
                dimension: "minecraft:overworld"
            })
            .unwrap(),
            fixture["physical"]
        );
        let receipt: CommandSubmission =
            serde_json::from_value(fixture["command_receipt"].clone()).unwrap();
        assert_eq!(receipt.submitted_changes, command.len());
        let receipt: PhysicalSubmission =
            serde_json::from_value(fixture["physical_receipt"].clone()).unwrap();
        assert_eq!(receipt.placed_changes, physical.len());
        assert!(
            serde_json::from_value::<CommandSubmission>(serde_json::json!({"submitted_changes":1}))
                .is_err()
        );
        for value in fixture["invalid_states"].as_array().unwrap() {
            assert!(serde_json::from_value::<NativeBlockState>(value.clone()).is_err());
        }
    }
}
