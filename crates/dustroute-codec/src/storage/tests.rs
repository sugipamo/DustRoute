use super::*;

#[test]
fn byte_limit_and_schema_errors_are_typed() {
    assert!(matches!(
        encode("test.v1", &"x".repeat(100), 64),
        Err(Error::ByteLimit)
    ));
    let bytes = encode("test.v1", &42_u64, 4096).unwrap();
    assert!(matches!(
        decode::<u64>("other.v1", &bytes, 4096),
        Err(Error::Schema)
    ));
    assert!(matches!(
        decode::<u64>("test.v1", &bytes, 1),
        Err(Error::ByteLimit)
    ));
}
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Fact {
    Empty,
    Scalar(u64),
    Pair(i32, String),
    Fields { yes: bool, data: Vec<u8> },
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum Tagged {
    Facts(Vec<Fact>),
    Nothing,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct History {
    facts: Tagged,
    properties: BTreeMap<String, Option<(u64, f64)>>,
}

#[test]
fn roundtrips_typed_nested_tagged_and_external_variants() {
    let data = History {
        facts: Tagged::Facts(vec![
            Fact::Empty,
            Fact::Scalar(u64::MAX),
            Fact::Pair(-5, "観測".into()),
            Fact::Fields {
                yes: true,
                data: vec![1, 2, 255],
            },
        ]),
        properties: BTreeMap::from([("a".into(), None), ("b".into(), Some((0, -0.0)))]),
    };
    let bytes = encode("history.v1", &data, 4096).unwrap();
    assert_eq!(decode::<History>("history.v1", &bytes, 4096).unwrap(), data);
    assert!(!bytes.starts_with(b"{"));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Historical {
    Fact { fact: Fact },
    Other { other: String },
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Flattened {
    id: u16,
    #[serde(flatten)]
    value: Historical,
}
#[test]
fn handles_flatten_and_untagged_historical_records_without_json() {
    let record = Flattened {
        id: 42,
        value: Historical::Fact {
            fact: Fact::Fields {
                yes: true,
                data: vec![],
            },
        },
    };
    let bytes = encode("flatten.v1", &record, 4096).unwrap();
    assert_eq!(
        decode::<Flattened>("flatten.v1", &bytes, 4096).unwrap(),
        record
    );
}

#[test]
fn rejects_old_wrong_schema_trailing_truncated_and_overlarge_records() {
    let bytes = encode("example.v1", &Fact::Scalar(12), 4096).unwrap();
    assert!(decode::<Fact>("example.v2", &bytes, 4096).is_err());
    assert!(decode::<Fact>("example.v1", b"{\"value\":12}", 4096).is_err());
    for end in 0..bytes.len() {
        assert!(decode::<Fact>("example.v1", &bytes[..end], 4096).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode::<Fact>("example.v1", &trailing, 4096).is_err());
    assert!(decode::<Fact>("example.v1", &bytes, bytes.len() - 1).is_err());
    assert!(encode("example.v1", &Fact::Scalar(12), bytes.len() - 1).is_err());
}

#[derive(Serialize, Deserialize)]
struct Nest {
    next: Option<Box<Nest>>,
}
#[test]
fn rejects_excessive_depth_and_untrusted_lengths() {
    let mut value = Nest { next: None };
    for _ in 0..128 {
        value = Nest {
            next: Some(Box::new(value)),
        };
    }
    assert!(encode("nest.v1", &value, 65536).is_err());
    let mut bytes = encode("text.v1", &"a", 4096).unwrap();
    let len_offset = bytes.len() - 9;
    bytes[len_offset..len_offset + 8].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(decode::<String>("text.v1", &bytes, 4096).is_err());
}

#[test]
fn refuses_nonfinite_numbers_before_durable_output() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(encode("float.v1", &value, 4096).is_err());
        let mut bytes = encode("float.v1", &1.0_f64, 4096).unwrap();
        let offset = bytes.len() - 8;
        bytes[offset..].copy_from_slice(&value.to_bits().to_be_bytes());
        assert!(decode::<f64>("float.v1", &bytes, 4096).is_err());
    }
}

#[test]
fn rejects_duplicate_encoded_keys_in_untrusted_input() {
    let data = BTreeMap::from([("a", 1_u8), ("b", 2_u8)]);
    let mut bytes = encode("map.v1", &data, 4096).unwrap();
    let needle = [15, 0, 0, 0, 0, 0, 0, 0, 1, b'b'];
    let index = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    bytes[index + needle.len() - 1] = b'a';
    assert!(decode::<BTreeMap<String, u8>>("map.v1", &bytes, 4096).is_err());
}

#[test]
fn rejects_excessive_value_counts_before_durable_output() {
    let values = vec![0_u8; 1_048_577];
    assert!(encode("values.v1", &values, 4 * 1024 * 1024).is_err());
}
