//! Received Minecraft text. Wire JSON is an external protocol payload, not a
//! general-purpose internal message, registry, scene or operation capability.
use serde::Serialize;
use std::collections::BTreeMap;

/// Opaque Java 1.16.1 text supplied by a server packet.
///
/// Rendering and JSON interpretation are caller boundary concerns. This wrapper
/// preserves received bytes, including unsupported text extensions, without
/// parsing a generic JSON tree or using them as internal control messages.
/// A default value means the packet did not supply text, rather than empty text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtocolText {
    encoded: Option<String>,
}
impl ProtocolText {
    pub(crate) fn received(encoded: String) -> Self {
        Self {
            encoded: Some(encoded),
        }
    }
    /// Returns exactly the server's external wire JSON, when supplied.
    /// No validation, rendering or authority is implied by this payload.
    pub fn as_wire_json(&self) -> Option<&str> {
        self.encoded.as_deref()
    }
}

/// Connection closure reported locally or by the Java 1.16.1 server.
/// A local diagnostic is never interpreted as a server text component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisconnectReason {
    /// Locally produced connection-lifetime diagnostic.
    Local(&'static str),
    /// External text supplied by the server's disconnect packet.
    Server(ProtocolText),
}
impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(reason) => f.write_str(reason),
            Self::Server(text) => {
                f.write_str(text.as_wire_json().unwrap_or("unavailable server text"))
            }
        }
    }
}

/// Received network-NBT data used by Java 1.21.11 text components.
/// Integer width, list element tag and native array kinds remain distinguishable;
/// this is native protocol data and cannot grant an operation capability.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "tag", content = "value", rename_all = "snake_case")]
pub enum TextNbt {
    /// Signed 8-bit integer tag.
    Byte(i8),
    /// Signed 16-bit integer tag.
    Short(i16),
    /// Signed 32-bit integer tag.
    Int(i32),
    /// Signed 64-bit integer tag.
    Long(i64),
    /// Finite 32-bit floating point tag.
    Float(f32),
    /// Finite 64-bit floating point tag.
    Double(f64),
    /// Native signed byte array.
    ByteArray(Vec<i8>),
    /// Native string tag.
    String(String),
    /// Homogeneous native list, including its declared element tag when empty.
    List {
        /// Wire element kind (0 may only describe an empty list).
        element_tag: u8,
        /// Received list elements in wire order.
        elements: Vec<TextNbt>,
    },
    /// Native named fields. Repeated names retain the last value, as in the
    /// established component projection; each received field still uses budget.
    Compound(BTreeMap<String, TextNbt>),
    /// Native signed 32-bit integer array.
    IntArray(Vec<i32>),
    /// Native signed 64-bit integer array.
    LongArray(Vec<i64>),
}
impl TextNbt {
    fn is_empty_array(&self) -> bool {
        match self {
            Self::List { elements, .. } => elements.is_empty(),
            Self::ByteArray(values) => values.is_empty(),
            Self::IntArray(values) => values.is_empty(),
            Self::LongArray(values) => values.is_empty(),
            _ => false,
        }
    }
    /// Exact literal text only. Translation, concatenation and unknown shapes
    /// remain unavailable; styles or click events are never executed.
    pub fn literal_text(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            Self::Compound(fields)
                if !fields.contains_key("translate")
                    && fields.get("extra").is_none_or(Self::is_empty_array) =>
            {
                match fields.get("text")? {
                    Self::String(text) => Some(text),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_wire_text_and_received_empty_payload_are_distinct() {
        assert_eq!(ProtocolText::default().as_wire_json(), None);
        assert_eq!(
            ProtocolText::received(String::new()).as_wire_json(),
            Some("")
        );
        let received = ProtocolText::received(
            r#"{"translate":"chat.type.text","with":["name","hello"]}"#.into(),
        );
        assert_eq!(
            received.as_wire_json(),
            Some(r#"{"translate":"chat.type.text","with":["name","hello"]}"#)
        );
    }
    #[test]
    fn translations_concatenations_and_non_array_extra_cannot_masquerade_as_literals() {
        let mut fields = BTreeMap::from([("text".into(), TextNbt::String("hello".into()))]);
        assert_eq!(
            TextNbt::Compound(fields.clone()).literal_text(),
            Some("hello")
        );
        fields.insert("translate".into(), TextNbt::String("native.key".into()));
        assert_eq!(TextNbt::Compound(fields.clone()).literal_text(), None);
        fields.remove("translate");
        for extra in [
            TextNbt::String(String::new()),
            TextNbt::List {
                element_tag: 8,
                elements: vec![TextNbt::String("world".into())],
            },
        ] {
            fields.insert("extra".into(), extra);
            assert_eq!(TextNbt::Compound(fields.clone()).literal_text(), None);
        }
        fields.insert(
            "extra".into(),
            TextNbt::List {
                element_tag: 0,
                elements: vec![],
            },
        );
        assert_eq!(TextNbt::Compound(fields).literal_text(), Some("hello"));
        assert_eq!(TextNbt::Byte(0).literal_text(), None);
    }
}
