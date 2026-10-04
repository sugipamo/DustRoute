//! Bounded network-NBT projection for received text components. This preserves
//! translation keys and arguments; it does not execute or render click events.
use super::wire::Reader;
use crate::text_component::TextNbt;
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;

pub(super) fn read(r: &mut Reader<'_>) -> Result<TextNbt> {
    let kind = r.u8()?;
    value(r, kind, 0, &mut 16384)
}
fn value(r: &mut Reader<'_>, kind: u8, depth: usize, budget: &mut usize) -> Result<TextNbt> {
    if depth > 32 || *budget == 0 {
        bail!("text component limit");
    }
    *budget -= 1;
    Ok(match kind {
        1 => TextNbt::Byte(r.u8()? as i8),
        2 => TextNbt::Short(r.u16()? as i16),
        3 => TextNbt::Int(r.i32()?),
        4 => TextNbt::Long(r.u64()? as i64),
        5 => TextNbt::Float(r.f32()?),
        6 => TextNbt::Double(r.f64()?),
        8 => TextNbt::String(r.nbt_string()?),
        7 | 9 | 11 | 12 => {
            let item = if kind == 9 {
                r.u8()?
            } else {
                match kind {
                    7 => 1,
                    11 => 3,
                    _ => 4,
                }
            };
            let count = usize::try_from(r.i32()?).context("negative component list")?;
            if count > *budget || (item == 0 && count != 0) {
                bail!("component list limit");
            }
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(value(r, item, depth + 1, budget)?);
            }
            match kind {
                7 => TextNbt::ByteArray(
                    values
                        .into_iter()
                        .map(|v| match v {
                            TextNbt::Byte(v) => v,
                            _ => unreachable!(),
                        })
                        .collect(),
                ),
                11 => TextNbt::IntArray(
                    values
                        .into_iter()
                        .map(|v| match v {
                            TextNbt::Int(v) => v,
                            _ => unreachable!(),
                        })
                        .collect(),
                ),
                12 => TextNbt::LongArray(
                    values
                        .into_iter()
                        .map(|v| match v {
                            TextNbt::Long(v) => v,
                            _ => unreachable!(),
                        })
                        .collect(),
                ),
                _ => TextNbt::List {
                    element_tag: item,
                    elements: values,
                },
            }
        }
        10 => {
            let mut values = BTreeMap::new();
            while let Some((item, key)) = r.nbt_field()? {
                values.insert(key, value(r, item, depth + 1, budget)?);
            }
            TextNbt::Compound(values)
        }
        _ => bail!("invalid text component NBT tag"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_literal_components_and_rejects_truncation_and_depth() {
        let data = b"\x0a\x08\x00\x04text\x00\x05hello\x00";
        assert_eq!(
            read(&mut Reader::new(data)).unwrap(),
            TextNbt::Compound(BTreeMap::from([(
                "text".into(),
                TextNbt::String("hello".into())
            )]))
        );
        for end in 0..data.len() {
            assert!(read(&mut Reader::new(&data[..end])).is_err());
        }
        assert!(read(&mut Reader::new(&[9, 8, 0x7f, 0xff, 0xff, 0xff])).is_err());
        let mut deep = vec![10];
        for _ in 0..34 {
            deep.extend([10, 0, 0]);
        }
        assert!(read(&mut Reader::new(&deep)).is_err());
    }
    #[test]
    fn native_kinds_arrays_and_empty_list_tags_are_not_erased() {
        let cases = [
            (vec![1, 255], TextNbt::Byte(-1)),
            (vec![2, 255, 254], TextNbt::Short(-2)),
            (
                [vec![3], (-3i32).to_be_bytes().to_vec()].concat(),
                TextNbt::Int(-3),
            ),
            (
                [vec![4], (-4i64).to_be_bytes().to_vec()].concat(),
                TextNbt::Long(-4),
            ),
            (
                [vec![5], 1.25f32.to_be_bytes().to_vec()].concat(),
                TextNbt::Float(1.25),
            ),
            (
                [vec![6], (-2.5f64).to_be_bytes().to_vec()].concat(),
                TextNbt::Double(-2.5),
            ),
            (vec![7, 0, 0, 0, 2, 255, 1], TextNbt::ByteArray(vec![-1, 1])),
            (
                vec![9, 0, 0, 0, 0, 0],
                TextNbt::List {
                    element_tag: 0,
                    elements: vec![],
                },
            ),
            (
                vec![9, 8, 0, 0, 0, 0],
                TextNbt::List {
                    element_tag: 8,
                    elements: vec![],
                },
            ),
            (
                vec![9, 2, 0, 0, 0, 1, 255, 254],
                TextNbt::List {
                    element_tag: 2,
                    elements: vec![TextNbt::Short(-2)],
                },
            ),
            (
                [vec![11, 0, 0, 0, 1], (-7i32).to_be_bytes().to_vec()].concat(),
                TextNbt::IntArray(vec![-7]),
            ),
            (
                [vec![12, 0, 0, 0, 1], (-8i64).to_be_bytes().to_vec()].concat(),
                TextNbt::LongArray(vec![-8]),
            ),
        ];
        for (data, expected) in cases {
            let mut r = Reader::new(&data);
            assert_eq!(read(&mut r).unwrap(), expected);
            r.end().unwrap();
            for end in 0..data.len() {
                assert!(
                    read(&mut Reader::new(&data[..end])).is_err(),
                    "truncated {expected:?} at {end}"
                );
            }
        }
    }
    #[test]
    fn duplicate_fields_still_consume_budget_and_invalid_lists_are_rejected() {
        let data = b"\x0a\x01\x00\x01a\x01\x01\x00\x01a\x02\x00";
        assert_eq!(
            read(&mut Reader::new(data)).unwrap(),
            TextNbt::Compound(BTreeMap::from([("a".into(), TextNbt::Byte(2))]))
        );
        assert!(value(&mut Reader::new(&data[1..]), 10, 0, &mut 2).is_err());
        for data in [
            vec![0],
            vec![13],
            vec![9, 0, 0, 0, 0, 1],
            vec![9, 8, 255, 255, 255, 255],
            vec![9, 13, 0, 0, 0, 1, 0],
        ] {
            assert!(read(&mut Reader::new(&data)).is_err());
        }
        let mut over = vec![7];
        over.extend(16384i32.to_be_bytes());
        over.resize(over.len() + 16384, 0);
        assert!(read(&mut Reader::new(&over)).is_err());
        let mut exact = vec![7];
        exact.extend(16383i32.to_be_bytes());
        exact.resize(exact.len() + 16383, 0);
        assert!(
            matches!(read(&mut Reader::new(&exact)).unwrap(), TextNbt::ByteArray(values) if values.len() == 16383)
        );
    }
}
