//! Read-only session projections. These cannot reconstruct a client, selection,
//! active operation or execution authority from an MCP response.
use crate::bridge::VisiblePlayer;
use crate::failure::{CauseResponse, FailureCause};
use crate::operations::mutation::{Success, UnrecordedFailure};
use dustroute_physical::Pos;
use dustroute_translate::world_reverse::RegionBounds;
use serde::{Serialize, Serializer};

#[derive(Serialize)]
pub(super) struct VisiblePlayers<'a> {
    #[serde(flatten)]
    pub outcome: VisibilityOutcome<'a>,
    pub players: Vec<VisiblePlayer>,
    pub assist_player: &'a Option<String>,
    pub reacquire_error: Option<DiagnosticText<'a>>,
}
#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum VisibilityOutcome<'a> {
    Visible { ok: Success },
    Failed(CauseResponse<'a>),
}
pub(super) struct DiagnosticText<'a>(pub &'a FailureCause);
impl Serialize for DiagnosticText<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self.0)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Corner {
    First,
    Second,
}
#[derive(Serialize)]
pub(super) struct MarkedCorner {
    pub ok: Success,
    pub corner: Corner,
    pub position: Pos,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<RegionBounds>,
}
#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum CornerResponse {
    Marked(MarkedCorner),
    Failed(UnrecordedFailure),
}
