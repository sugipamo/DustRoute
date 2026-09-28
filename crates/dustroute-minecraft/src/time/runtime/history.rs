//! Bounded position-owned histories. They survive block replacement; a device
//! definition selects the namespace, inclusive window and saturation threshold.
use super::RuntimeError;
use crate::{Pos, Region};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPolicy {
    pub window: u16,
    pub threshold: u8,
}
impl HistoryPolicy {
    pub const fn valid(self) -> bool {
        self.window > 0 && self.threshold > 0 && self.threshold <= 64
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRule {
    pub key: String,
    pub policy: HistoryPolicy,
}
impl HistoryRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_empty() || self.key.len() > 128 || !self.policy.valid() {
            return Err("invalid bounded history rule".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecentHistory {
    pub(crate) policy: HistoryPolicy,
    pub(crate) times: VecDeque<u64>,
}
pub(crate) type Histories = BTreeMap<(String, Pos), RecentHistory>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum HistoryEffect {
    Prune { rule: HistoryRule },
    Record { rule: HistoryRule, position: Pos },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HistoryChange {
    pub key: String,
    pub position: Pos,
    pub before: Option<RecentHistory>,
    pub after: Option<RecentHistory>,
}

pub(crate) fn count(
    histories: &Histories,
    rule: &HistoryRule,
    pos: Pos,
    now: u64,
) -> Result<u8, RuntimeError> {
    rule.validate().map_err(RuntimeError::Invalid)?;
    let Some(history) = histories.get(&(rule.key.clone(), pos)) else {
        return Ok(0);
    };
    if history.policy != rule.policy || history.times.iter().any(|t| *t > now) {
        return Err(RuntimeError::Invalid(
            "inconsistent position history".into(),
        ));
    }
    Ok(history
        .times
        .iter()
        .filter(|t| now - **t <= rule.policy.window.into())
        .count() as u8)
}

pub(crate) fn apply(
    histories: &mut Histories,
    effects: Vec<HistoryEffect>,
    region: Region,
    now: u64,
) -> Result<(), RuntimeError> {
    for effect in effects {
        let rule = match &effect {
            HistoryEffect::Prune { rule } | HistoryEffect::Record { rule, .. } => rule,
        };
        rule.validate().map_err(RuntimeError::Invalid)?;
        if histories
            .iter()
            .any(|((key, _), history)| key == &rule.key && history.policy != rule.policy)
        {
            return Err(RuntimeError::Invalid(
                "conflicting world history policies".into(),
            ));
        }
        match effect {
            HistoryEffect::Prune { rule } => {
                histories.retain(|(key, _), h| {
                    if key == &rule.key {
                        h.times
                            .retain(|t| now.saturating_sub(*t) <= rule.policy.window.into());
                    }
                    !h.times.is_empty()
                });
            }
            HistoryEffect::Record { rule, position } => {
                if !region.contains(position) {
                    return Err(RuntimeError::UnknownSpace(position));
                }
                let history =
                    histories
                        .entry((rule.key, position))
                        .or_insert_with(|| RecentHistory {
                            policy: rule.policy,
                            times: VecDeque::new(),
                        });
                history.times.push_back(now);
                // Older entries cannot affect a future threshold crossing: all
                // retained entries expire no earlier than the discarded ones.
                while history.times.len() > usize::from(rule.policy.threshold) {
                    history.times.pop_front();
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclusive_expiry_saturation_and_namespace_isolation() {
        let a = HistoryRule {
            key: "a".into(),
            policy: HistoryPolicy {
                window: 60,
                threshold: 8,
            },
        };
        let b = HistoryRule {
            key: "b".into(),
            policy: HistoryPolicy {
                window: 90,
                threshold: 8,
            },
        };
        let p = Pos::new(0, 0, 0);
        let q = Pos::new(1, 0, 0);
        let region = Region::new(p, q);
        let mut histories = Histories::new();
        apply(
            &mut histories,
            vec![
                HistoryEffect::Record {
                    rule: b.clone(),
                    position: p,
                },
                HistoryEffect::Record {
                    rule: a.clone(),
                    position: q,
                },
            ],
            region,
            0,
        )
        .unwrap();
        for time in 0..12 {
            apply(
                &mut histories,
                vec![HistoryEffect::Record {
                    rule: a.clone(),
                    position: p,
                }],
                region,
                time,
            )
            .unwrap();
        }
        assert_eq!(count(&histories, &a, p, 64).unwrap(), 8); // retained oldest is tick 4
        assert_eq!(count(&histories, &a, p, 65).unwrap(), 7);
        assert_eq!(count(&histories, &a, q, 60).unwrap(), 1);
        assert_eq!(count(&histories, &a, q, 61).unwrap(), 0);
        apply(
            &mut histories,
            vec![HistoryEffect::Prune { rule: a.clone() }],
            region,
            72,
        )
        .unwrap();
        assert_eq!(histories.len(), 1);
        assert_eq!(count(&histories, &b, p, 72).unwrap(), 1);
        let mut conflict = b.clone();
        conflict.policy.window = 10;
        assert!(count(&histories, &conflict, p, 72).is_err());
    }
}
