//! Scalar Minecraft state properties. Decode failures are kept only long
//! enough for the owning observation to add its coordinate; rejected input
//! never becomes a block state or a complete transition trace.
use crate::world::Pos;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ObservedProperty {
    Text(String),
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
}
impl Display for ObservedProperty {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(v) => f.write_str(v),
            Self::Boolean(v) => Display::fmt(v, f),
            Self::Signed(v) => Display::fmt(v, f),
            Self::Unsigned(v) => Display::fmt(v, f),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectedPropertyKind {
    Null,
    Sequence,
    Object,
    NonIntegerNumber,
    NonObjectProperties,
}
#[derive(Debug)]
pub struct InvalidObservedProperty {
    pub property: Option<String>,
    pub kind: RejectedPropertyKind,
    pub position: Option<Pos>,
    pub state_slot: Option<&'static str>,
}
impl Display for InvalidObservedProperty {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "invalid observed properties")?;
        if let Some(slot) = self.state_slot {
            write!(f, " in {slot}")?;
        }
        if let Some(pos) = self.position {
            write!(f, " at {pos:?}")?;
        }
        if let Some(key) = &self.property {
            write!(f, ".{key}")?;
        }
        write!(
            f,
            ": {:?}; expected an object with string, boolean or 64-bit integer values",
            self.kind
        )
    }
}
impl std::error::Error for InvalidObservedProperty {}

// This is not a generic value tree: unsupported structures are consumed and
// discarded, retaining only their rejected kind for a contextual error.
struct PropertyCandidate(Result<ObservedProperty, RejectedPropertyKind>);
impl<'de> Deserialize<'de> for PropertyCandidate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Scalar;
        impl<'de> Visitor<'de> for Scalar {
            type Value = PropertyCandidate;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a Minecraft string, boolean or 64-bit integer property")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                self.visit_string(v.into())
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Ok(ObservedProperty::Text(v))))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Ok(ObservedProperty::Boolean(v))))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Ok(ObservedProperty::Signed(v))))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Ok(ObservedProperty::Unsigned(v))))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Err(
                    RejectedPropertyKind::NonIntegerNumber,
                )))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(PropertyCandidate(Err(RejectedPropertyKind::Null)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_element::<IgnoredAny>()?.is_some() {}
                Ok(PropertyCandidate(Err(RejectedPropertyKind::Sequence)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(PropertyCandidate(Err(RejectedPropertyKind::Object)))
            }
        }
        d.deserialize_any(Scalar)
    }
}
impl<'de> Deserialize<'de> for ObservedProperty {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        PropertyCandidate::deserialize(d)?.0.map_err(|kind| {
            serde::de::Error::custom(InvalidObservedProperty {
                property: None,
                kind,
                position: None,
                state_slot: None,
            })
        })
    }
}

pub(crate) struct PropertyInput(
    Result<BTreeMap<String, ObservedProperty>, InvalidObservedProperty>,
);
impl Default for PropertyInput {
    fn default() -> Self {
        Self(Ok(BTreeMap::new()))
    }
}
impl PropertyInput {
    pub(crate) fn finish(
        self,
        position: Option<Pos>,
        state_slot: &'static str,
    ) -> Result<BTreeMap<String, ObservedProperty>, InvalidObservedProperty> {
        self.0.map_err(|mut error| {
            error.position = position;
            error.state_slot = Some(state_slot);
            error
        })
    }
}
impl<'de> Deserialize<'de> for PropertyInput {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Properties;
        fn rejected(kind: RejectedPropertyKind) -> PropertyInput {
            PropertyInput(Err(InvalidObservedProperty {
                property: None,
                kind,
                position: None,
                state_slot: None,
            }))
        }
        impl<'de> Visitor<'de> for Properties {
            type Value = PropertyInput;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("an object of Minecraft state properties")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                let mut invalid = None;
                while let Some(key) = a.next_key::<String>()? {
                    match a.next_value::<PropertyCandidate>()?.0 {
                        Ok(value) if invalid.is_none() => {
                            values.insert(key, value);
                        }
                        Err(kind) if invalid.is_none() => {
                            invalid = Some(InvalidObservedProperty {
                                property: Some(key),
                                kind,
                                position: None,
                                state_slot: None,
                            });
                        }
                        _ => {}
                    }
                }
                Ok(PropertyInput(invalid.map_or(Ok(values), Err)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_element::<IgnoredAny>()?.is_some() {}
                Ok(rejected(RejectedPropertyKind::Sequence))
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::NonObjectProperties))
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::NonObjectProperties))
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::NonObjectProperties))
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::NonObjectProperties))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(rejected(RejectedPropertyKind::NonObjectProperties))
            }
        }
        d.deserialize_any(Properties)
    }
}
