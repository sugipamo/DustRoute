use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_model::Pos;
use dustroute_translate::RegionBounds;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionSelection {
    #[default]
    Empty,
    FirstCorner(Pos),
    Complete {
        first: Pos,
        second: Pos,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectionSession {
    pub player: String,
    pub selection: RegionSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionError {
    MissingFirstCorner,
    Incomplete,
}

impl Display for SelectionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFirstCorner => f.write_str("mark the first corner before the second"),
            Self::Incomplete => f.write_str("the region selection is incomplete"),
        }
    }
}

impl Error for SelectionError {}

impl SelectionSession {
    #[must_use]
    pub fn new(player: impl Into<String>) -> Self {
        Self {
            player: player.into(),
            selection: RegionSelection::Empty,
        }
    }

    pub fn mark_first(&mut self, pos: Pos) {
        self.selection = RegionSelection::FirstCorner(pos);
    }

    pub fn set_bounds(&mut self, bounds: RegionBounds) {
        self.selection = RegionSelection::Complete {
            first: bounds.min,
            second: bounds.max,
        };
    }

    pub fn mark_second(&mut self, pos: Pos) -> Result<RegionBounds, SelectionError> {
        let RegionSelection::FirstCorner(first) = self.selection else {
            return Err(SelectionError::MissingFirstCorner);
        };
        self.selection = RegionSelection::Complete { first, second: pos };
        Ok(RegionBounds::new(first, pos))
    }

    pub fn bounds(&self) -> Result<RegionBounds, SelectionError> {
        match self.selection {
            RegionSelection::Complete { first, second } => Ok(RegionBounds::new(first, second)),
            _ => Err(SelectionError::Incomplete),
        }
    }

    pub fn clear(&mut self) {
        self.selection = RegionSelection::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_two_gaze_corners() {
        let mut session = SelectionSession::new("builder");
        session.mark_first(Pos::new(10, 70, 5));
        let bounds = session.mark_second(Pos::new(2, 64, 12)).unwrap();
        assert_eq!(bounds.min, Pos::new(2, 64, 5));
        assert_eq!(bounds.max, Pos::new(10, 70, 12));
    }
}
