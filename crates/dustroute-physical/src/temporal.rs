use serde::{Deserialize, Serialize};

use crate::{BlockKind, ComponentId, PhysicalScene, UpdateModel, behavior_profile};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum TemporalRequirement {
    SteadyStateSafe,
    OrderedUpdates,
    ScheduledTicks,
    BlockEvents,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemporalReason {
    IncompleteObservation,
    OrderSensitiveComponent {
        component: ComponentId,
        block: BlockKind,
    },
    ScheduledTickComponent {
        component: ComponentId,
        block: BlockKind,
    },
    BlockEventComponent {
        component: ComponentId,
        block: BlockKind,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemporalAssessment {
    pub requirement: TemporalRequirement,
    pub reasons: Vec<TemporalReason>,
}

impl PhysicalScene {
    #[must_use]
    pub fn temporal_assessment(&self) -> TemporalAssessment {
        if !self.observation.is_complete() {
            return TemporalAssessment {
                requirement: TemporalRequirement::Unsupported,
                reasons: vec![TemporalReason::IncompleteObservation],
            };
        }
        let mut requirement = TemporalRequirement::SteadyStateSafe;
        let mut reasons = Vec::new();
        for component in &self.components {
            let profile = behavior_profile(component.block.kind);
            match profile.update_model {
                UpdateModel::Passive if !profile.order_sensitive => {}
                UpdateModel::Passive
                | UpdateModel::ImmediateNeighborChain
                | UpdateModel::UserInteraction => {
                    requirement = requirement.max(TemporalRequirement::OrderedUpdates);
                    reasons.push(TemporalReason::OrderSensitiveComponent {
                        component: component.id,
                        block: component.block.kind,
                    });
                }
                UpdateModel::ScheduledBlockTick => {
                    requirement = requirement.max(TemporalRequirement::ScheduledTicks);
                    reasons.push(TemporalReason::ScheduledTickComponent {
                        component: component.id,
                        block: component.block.kind,
                    });
                }
                UpdateModel::BlockEvent => {
                    requirement = requirement.max(TemporalRequirement::BlockEvents);
                    reasons.push(TemporalReason::BlockEventComponent {
                        component: component.id,
                        block: component.block.kind,
                    });
                }
            }
        }
        TemporalAssessment {
            requirement,
            reasons,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Block, Observation, ObservationFrontier, PhysicalComponent, Pos, SceneBounds,
        VerifiedTopology,
    };

    use super::*;

    fn scene(kind: BlockKind) -> PhysicalScene {
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: Block::new(kind),
            }],
            [],
        );
        PhysicalScene::from_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(-1, 0, -1), Pos::new(1, 2, 1)),
            ),
            &topology,
        )
    }

    #[test]
    fn classifies_ordered_scheduled_and_block_event_components() {
        assert_eq!(
            scene(BlockKind::RedstoneWire)
                .temporal_assessment()
                .requirement,
            TemporalRequirement::OrderedUpdates
        );
        assert_eq!(
            scene(BlockKind::Repeater).temporal_assessment().requirement,
            TemporalRequirement::ScheduledTicks
        );
        assert_eq!(
            scene(BlockKind::Piston).temporal_assessment().requirement,
            TemporalRequirement::BlockEvents
        );
    }

    #[test]
    fn incomplete_observation_is_unsupported() {
        let mut scene = scene(BlockKind::RedstoneWire);
        scene.observation.frontier.push(ObservationFrontier {
            position: Pos::new(1, 1, 0),
            direction: crate::Facing::East,
            reason: crate::FrontierReason::ScanLimitReached,
        });
        assert_eq!(
            scene.temporal_assessment().requirement,
            TemporalRequirement::Unsupported
        );
    }
}
