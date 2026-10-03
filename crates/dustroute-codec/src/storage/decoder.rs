//! Direct serde decoding of tagged data. No JSON/value tree or domain objects.
use super::{Error, Result};
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

const MAX_DEPTH: usize = 128;
const MAX_VALUES: usize = 1_048_576;
pub(super) struct Decoder<'de> {
    bytes: &'de [u8],
    offset: usize,
    depth: usize,
    remaining: usize,
}
impl<'de> Decoder<'de> {
    pub fn new(bytes: &'de [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            depth: 0,
            remaining: MAX_VALUES,
        }
    }
    fn take(&mut self, count: usize) -> Result<&'de [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| Error("record length overflow".into()))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| Error("truncated record".into()))?;
        self.offset = end;
        Ok(bytes)
    }
    pub fn expect_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.take(bytes.len())? != bytes {
            return Err(Error("unsupported stored encoding".into()));
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<()> {
        if self.offset != self.bytes.len() {
            return Err(Error("trailing record data".into()));
        }
        Ok(())
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn peek(&self) -> Result<u8> {
        self.bytes
            .get(self.offset)
            .copied()
            .ok_or_else(|| Error("truncated record".into()))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("checked slice length"))
    }
    fn bytes_payload(&mut self) -> Result<&'de [u8]> {
        let len = usize::try_from(u64::from_be_bytes(self.array()?))
            .map_err(|_| Error("record length overflow".into()))?;
        self.take(len)
    }
    pub fn text_payload(&mut self) -> Result<&'de str> {
        std::str::from_utf8(self.bytes_payload()?).map_err(|_| Error("invalid record UTF-8".into()))
    }
    fn name(&mut self) -> Result<&'de str> {
        if self.byte()? != 15 {
            return Err(Error("invalid record name".into()));
        }
        self.text_payload()
    }
    fn enter(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH || self.remaining == 0 {
            return Err(Error("record depth/value limit exceeded".into()));
        }
        self.depth += 1;
        self.remaining -= 1;
        Ok(())
    }
    fn sequence<V: Visitor<'de>>(&mut self, visitor: V) -> Result<V::Value> {
        let value = visitor.visit_seq(Sequence(self))?;
        self.expect_bytes(&[0])?;
        Ok(value)
    }
    fn map<V: Visitor<'de>>(&mut self, visitor: V) -> Result<V::Value> {
        let mut access = Fields {
            decoder: self,
            pending: false,
            previous: None,
        };
        let value = visitor.visit_map(&mut access)?;
        if access.pending {
            return Err(Error("record map has no value".into()));
        }
        access.decoder.expect_bytes(&[0])?;
        Ok(value)
    }
}
impl<'de> de::Deserializer<'de> for &mut Decoder<'de> {
    type Error = Error;
    fn is_human_readable(&self) -> bool {
        false
    }
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.enter()?;
        let result = (|| match self.byte()? {
            1 => match self.byte()? {
                0 => visitor.visit_bool(false),
                1 => visitor.visit_bool(true),
                _ => Err(Error("invalid boolean".into())),
            },
            2 => visitor.visit_i8(i8::from_be_bytes(self.array()?)),
            3 => visitor.visit_i16(i16::from_be_bytes(self.array()?)),
            4 => visitor.visit_i32(i32::from_be_bytes(self.array()?)),
            5 => visitor.visit_i64(i64::from_be_bytes(self.array()?)),
            6 => visitor.visit_i128(i128::from_be_bytes(self.array()?)),
            7 => visitor.visit_u8(u8::from_be_bytes(self.array()?)),
            8 => visitor.visit_u16(u16::from_be_bytes(self.array()?)),
            9 => visitor.visit_u32(u32::from_be_bytes(self.array()?)),
            10 => visitor.visit_u64(u64::from_be_bytes(self.array()?)),
            11 => visitor.visit_u128(u128::from_be_bytes(self.array()?)),
            12 => {
                let value = f32::from_bits(u32::from_be_bytes(self.array()?));
                if !value.is_finite() {
                    return Err(Error("non-finite stored number".into()));
                }
                visitor.visit_f32(value)
            }
            13 => {
                let value = f64::from_bits(u64::from_be_bytes(self.array()?));
                if !value.is_finite() {
                    return Err(Error("non-finite stored number".into()));
                }
                visitor.visit_f64(value)
            }
            14 => visitor.visit_char(
                char::from_u32(u32::from_be_bytes(self.array()?))
                    .ok_or_else(|| Error("invalid char".into()))?,
            ),
            15 => visitor.visit_borrowed_str(self.text_payload()?),
            16 => visitor.visit_borrowed_bytes(self.bytes_payload()?),
            17 => visitor.visit_none(),
            18 => visitor.visit_some(&mut *self),
            19 => visitor.visit_unit(),
            20 => {
                self.name()?;
                visitor.visit_unit()
            }
            21 => {
                self.name()?;
                visitor.visit_borrowed_str(self.name()?)
            }
            tag @ (23 | 27 | 30) => {
                self.name()?;
                let variant = self.name()?;
                let mut entry = EnumEntry {
                    decoder: self,
                    variant: Some(variant),
                    tag,
                    pending: false,
                };
                let value = visitor.visit_map(&mut entry)?;
                if entry.variant.is_some() || entry.pending {
                    return Err(Error("incomplete enum payload".into()));
                }
                Ok(value)
            }
            22 => {
                self.name()?;
                visitor.visit_newtype_struct(&mut *self)
            }
            24 | 25 => self.sequence(visitor),
            26 => {
                self.name()?;
                self.sequence(visitor)
            }
            28 => self.map(visitor),
            29 => {
                self.name()?;
                self.map(visitor)
            }
            _ => Err(Error("invalid record tag".into())),
        })();
        self.depth -= 1;
        result
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        // Internally/adjacently tagged enums are maps; externally tagged Rust
        // variants have their own payload shapes and names.
        self.enter()?;
        let result = (|| {
            let tag = self.byte()?;
            if !matches!(tag, 21 | 23 | 27 | 30) {
                return Err(Error("expected typed enum".into()));
            }
            self.name()?;
            let variant = self.name()?;
            visitor.visit_enum(Variant {
                decoder: self,
                variant,
                tag,
            })
        })();
        self.depth -= 1;
        result
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
struct Sequence<'a, 'de>(&'a mut Decoder<'de>);
impl<'de> SeqAccess<'de> for Sequence<'_, 'de> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.0.peek()? == 0 {
            Ok(None)
        } else {
            seed.deserialize(&mut *self.0).map(Some)
        }
    }
    // Never trust an input count as a capacity hint.
}
struct Fields<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
    pending: bool,
    previous: Option<&'de [u8]>,
}
impl<'de> MapAccess<'de> for &mut Fields<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.pending {
            return Err(Error("record map has no value".into()));
        }
        if self.decoder.peek()? == 0 {
            return Ok(None);
        }
        let start = self.decoder.offset;
        let key = seed.deserialize(&mut *self.decoder)?;
        let bytes = &self.decoder.bytes[start..self.decoder.offset];
        if self.previous.is_some_and(|old| old >= bytes) {
            return Err(Error("duplicate or unordered record key".into()));
        }
        self.previous = Some(bytes);
        self.pending = true;
        Ok(Some(key))
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        if !self.pending {
            return Err(Error("record map value has no key".into()));
        }
        self.pending = false;
        seed.deserialize(&mut *self.decoder)
    }
}
// Serde's tagged/untagged buffering expects generic enum content as a
// one-entry map. Keep decoding direct and expose that view without a tree.
struct EnumEntry<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
    variant: Option<&'de str>,
    tag: u8,
    pending: bool,
}
impl<'de> MapAccess<'de> for &mut EnumEntry<'_, 'de> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.pending {
            return Err(Error("enum value not consumed".into()));
        }
        self.variant
            .take()
            .map(|variant| {
                self.pending = true;
                seed.deserialize(de::value::BorrowedStrDeserializer::<Error>::new(variant))
            })
            .transpose()
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        if !self.pending {
            return Err(Error("enum value has no key".into()));
        }
        self.pending = false;
        seed.deserialize(EnumPayload {
            decoder: self.decoder,
            tag: self.tag,
        })
    }
}
struct EnumPayload<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
    tag: u8,
}
impl<'de> de::Deserializer<'de> for EnumPayload<'_, 'de> {
    type Error = Error;
    fn is_human_readable(&self) -> bool {
        false
    }
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.tag {
            23 => de::Deserializer::deserialize_any(self.decoder, visitor),
            27 => self.decoder.sequence(visitor),
            30 => self.decoder.map(visitor),
            _ => Err(Error("invalid enum payload".into())),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}
struct Variant<'a, 'de> {
    decoder: &'a mut Decoder<'de>,
    variant: &'de str,
    tag: u8,
}
impl<'a, 'de> EnumAccess<'de> for Variant<'a, 'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let value = seed.deserialize(de::value::BorrowedStrDeserializer::<Error>::new(
            self.variant,
        ))?;
        Ok((value, self))
    }
}
impl<'de> VariantAccess<'de> for Variant<'_, 'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        if self.tag != 21 {
            return Err(Error("expected unit variant".into()));
        }
        Ok(())
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        if self.tag != 23 {
            return Err(Error("expected newtype variant".into()));
        }
        seed.deserialize(self.decoder)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        if self.tag != 27 {
            return Err(Error("expected tuple variant".into()));
        }
        self.decoder.sequence(visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        if self.tag != 30 {
            return Err(Error("expected struct variant".into()));
        }
        self.decoder.map(visitor)
    }
}
