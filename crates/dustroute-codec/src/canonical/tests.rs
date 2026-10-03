use super::*;
use serde::ser::{SerializeMap, SerializeSeq};
use std::collections::HashMap;

#[test]
fn primitive_bytes_pin_the_format_version_and_integer_width() {
    let mut expected = HEADER.to_vec();
    expected.extend([8, 0, 42]);
    assert_eq!(encode(&42u16).unwrap(), expected);
    assert_ne!(encode(&42u16).unwrap(), encode(&42u64).unwrap());
    assert_ne!(encode(&42u8).unwrap(), encode(&42i8).unwrap());
}

#[test]
fn framing_preserves_text_boundaries_nested_options_and_collection_types() {
    assert_ne!(encode(&("a", "bc")).unwrap(), encode(&("ab", "c")).unwrap());
    assert_ne!(
        encode(&Some(None::<u8>)).unwrap(),
        encode(&None::<Option<u8>>).unwrap()
    );
    assert_ne!(encode(&vec![1u8, 2]).unwrap(), encode(&(1u8, 2u8)).unwrap());
    assert_ne!(
        encode(&vec![vec![1u8], vec![2]]).unwrap(),
        encode(&vec![vec![1u8, 2]]).unwrap()
    );
    assert_ne!(
        encode(&vec![1u8, 2]).unwrap(),
        encode(&vec![2u8, 1]).unwrap()
    );
}

struct OrderedMap(Vec<(&'static str, u8)>);
impl Serialize for OrderedMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[test]
fn map_order_is_canonical_and_duplicate_serialized_keys_are_refused() {
    let forward = OrderedMap(vec![("a", 1), ("b", 2)]);
    let backward = OrderedMap(vec![("b", 2), ("a", 1)]);
    assert_eq!(encode(&forward).unwrap(), encode(&backward).unwrap());
    let random: HashMap<_, _> = backward.0.iter().copied().collect();
    assert_eq!(encode(&forward).unwrap(), encode(&random).unwrap());
    assert!(encode(&OrderedMap(vec![("a", 1), ("a", 2)])).is_err());
}

#[derive(Serialize)]
enum Choice {
    Idle,
    Scalar(u8),
    Pair(u8, u8),
    Named { a: u8, b: u8 },
}
#[derive(Serialize)]
struct Pair(u8, u8);

#[test]
fn named_types_variants_and_field_values_remain_distinct() {
    let keys = [
        encode(&Choice::Idle).unwrap(),
        encode(&Choice::Scalar(1)).unwrap(),
        encode(&Choice::Pair(1, 2)).unwrap(),
        encode(&Choice::Named { a: 1, b: 2 }).unwrap(),
        encode(&Choice::Named { a: 2, b: 1 }).unwrap(),
        encode(&Pair(1, 2)).unwrap(),
        encode(&("Pair", 1u8, 2u8)).unwrap(),
    ];
    for (i, key) in keys.iter().enumerate() {
        assert!(!keys[..i].contains(key));
    }
}

#[test]
fn floats_preserve_exact_bits_including_signed_zero_and_nan_payloads() {
    assert_ne!(encode(&0.0f64).unwrap(), encode(&-0.0f64).unwrap());
    assert_ne!(
        encode(&f64::from_bits(0x7ff8000000000001)).unwrap(),
        encode(&f64::from_bits(0x7ff8000000000002)).unwrap()
    );
    assert_ne!(encode(&f64::NAN).unwrap(), encode(&None::<f64>).unwrap());
}

struct WrongLength;
impl Serialize for WrongLength {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(1))?;
        seq.serialize_element(&1u8)?;
        seq.serialize_element(&2u8)?;
        seq.end()
    }
}
struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("injected write failure"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn failed_writes_and_malformed_serializers_cannot_return_a_key() {
    assert!(encode(&WrongLength).is_err());
    assert!(
        write(&mut FailedWriter, &42u8)
            .unwrap_err()
            .to_string()
            .contains("injected write failure")
    );
}
