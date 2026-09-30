//! Compiled support predicates and deferred rechecks. Queries consume relative
//! known-space reads supplied by the executor; they never invent absent data.
//! ```compile_fail,E0080
//! use dustroute_minecraft::physical::lifetime::*;
//! const BAD: DelayedSupport = DelayedSupport {
//!     query: SupportQuery::Any(&[]), delay: 0, priority: 3,
//! }.checked();
//! ```
use super::environment::{EnvironmentTag, has_tag, short_name};
use crate::{Block, Pos};

#[derive(Clone, Copy, Debug)]
pub enum SupportFact {
    SameIdentity,
    Tag(EnvironmentTag),
}
#[derive(Clone, Copy, Debug)]
pub enum SupportQuery {
    At { offset: Pos, fact: SupportFact },
    All(&'static [SupportQuery]),
    Any(&'static [SupportQuery]),
}
impl SupportQuery {
    const fn check(self, depth: usize) {
        assert!(depth < 16, "support query nesting exceeds 16");
        match self {
            Self::At { offset, .. } => {
                assert!(
                    offset.x >= -16
                        && offset.x <= 16
                        && offset.y >= -16
                        && offset.y <= 16
                        && offset.z >= -16
                        && offset.z <= 16,
                    "support query is not local"
                );
            }
            Self::All(items) | Self::Any(items) => {
                assert!(
                    !items.is_empty() && items.len() <= 16,
                    "bounded nonempty support query required"
                );
                let mut i = 0;
                while i < items.len() {
                    items[i].check(depth + 1);
                    i += 1;
                }
            }
        }
    }
    pub fn evaluate<E>(
        self,
        block: &Block,
        lookup: &mut impl FnMut(Pos) -> Result<Block, E>,
    ) -> Result<bool, E> {
        match self {
            Self::At { offset, fact } => {
                let other = lookup(offset)?;
                Ok(match fact {
                    SupportFact::Tag(tag) => has_tag(&other, tag),
                    SupportFact::SameIdentity => {
                        block.kind == other.kind
                            && block
                                .observed_name
                                .as_deref()
                                .map(short_name)
                                .zip(other.observed_name.as_deref().map(short_name))
                                .is_some_and(|(a, b)| a == b)
                    }
                })
            }
            Self::All(items) => {
                for q in items {
                    if !q.evaluate(block, lookup)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(items) => {
                for q in items {
                    if q.evaluate(block, lookup)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DelayedSupport {
    pub query: SupportQuery,
    pub delay: u64,
    pub priority: u8,
}
impl DelayedSupport {
    pub const fn checked(self) -> Self {
        self.query.check(0);
        assert!(
            self.delay > 0 && self.delay <= 200 && self.priority <= 6,
            "bounded delayed support tick required"
        );
        self
    }
}
