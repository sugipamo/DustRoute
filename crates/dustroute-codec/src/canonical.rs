//! Self-delimiting, versioned keys driven directly by Rust Serialize implementations.
//! Scalars have distinct tags and fixed-width big-endian payloads. Text/bytes are
//! length framed. Collections have an end marker; maps sort their encoded keys.
//! No deserializer is provided. Changing this format requires a new version.
use serde::Serialize;
use serde::ser::{self, Serializer};
use std::{fmt, io::Write};

const HEADER: &[u8] = b"dustroute.canonical-key.v1\0";
const END: u8 = 0;

#[derive(Debug)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
type Result<T> = std::result::Result<T, Error>;

pub fn encode(value: &(impl Serialize + ?Sized)) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write(&mut bytes, value)?;
    Ok(bytes)
}
pub fn write(writer: &mut impl Write, value: &(impl Serialize + ?Sized)) -> Result<()> {
    writer.write_all(HEADER)?;
    value.serialize(&mut Encoder(writer))
}

struct Encoder<W>(W);
impl<W: Write> Encoder<W> {
    fn tag(&mut self, tag: u8) -> Result<()> {
        self.0.write_all(&[tag]).map_err(Into::into)
    }
    fn bytes(&mut self, tag: u8, value: &[u8]) -> Result<()> {
        self.tag(tag)?;
        self.0.write_all(&(value.len() as u64).to_be_bytes())?;
        self.0.write_all(value)?;
        Ok(())
    }
    fn name(&mut self, name: &str) -> Result<()> {
        self.bytes(15, name.as_bytes())
    }
    fn collection(&mut self, tag: u8, length: Option<usize>) -> Result<Sequence<'_, W>> {
        self.tag(tag)?;
        Ok(self.sequence(length))
    }
    fn fields(&mut self, tag: u8, length: Option<usize>) -> Result<Fields<'_, W>> {
        self.tag(tag)?;
        Ok(self.dictionary(length))
    }
    fn sequence(&mut self, length: Option<usize>) -> Sequence<'_, W> {
        Sequence {
            encoder: self,
            expected: length,
            count: 0,
        }
    }
    fn dictionary(&mut self, length: Option<usize>) -> Fields<'_, W> {
        Fields {
            encoder: self,
            expected: length,
            entries: Vec::new(),
            pending_key: None,
        }
    }
}

macro_rules! scalar {
    ($($method:ident($ty:ty) => $tag:literal),* $(,)?) => {$(
        fn $method(self, value: $ty) -> Result<()> {
            self.tag($tag)?;
            self.0.write_all(&value.to_be_bytes())?;
            Ok(())
        }
    )*};
}
impl<'a, W: Write> Serializer for &'a mut Encoder<W> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Sequence<'a, W>;
    type SerializeTuple = Sequence<'a, W>;
    type SerializeTupleStruct = Sequence<'a, W>;
    type SerializeTupleVariant = Sequence<'a, W>;
    type SerializeMap = Fields<'a, W>;
    type SerializeStruct = Fields<'a, W>;
    type SerializeStructVariant = Fields<'a, W>;
    scalar! {
        serialize_i8(i8) => 2, serialize_i16(i16) => 3, serialize_i32(i32) => 4,
        serialize_i64(i64) => 5, serialize_i128(i128) => 6,
        serialize_u8(u8) => 7, serialize_u16(u16) => 8, serialize_u32(u32) => 9,
        serialize_u64(u64) => 10, serialize_u128(u128) => 11,
    }
    fn serialize_bool(self, value: bool) -> Result<()> {
        self.tag(1)?;
        self.tag(u8::from(value))
    }
    fn serialize_f32(self, value: f32) -> Result<()> {
        self.tag(12)?;
        self.0.write_all(&value.to_bits().to_be_bytes())?;
        Ok(())
    }
    fn serialize_f64(self, value: f64) -> Result<()> {
        self.tag(13)?;
        self.0.write_all(&value.to_bits().to_be_bytes())?;
        Ok(())
    }
    fn serialize_char(self, value: char) -> Result<()> {
        self.tag(14)?;
        self.0.write_all(&(value as u32).to_be_bytes())?;
        Ok(())
    }
    fn serialize_str(self, value: &str) -> Result<()> {
        self.name(value)
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        self.bytes(16, value)
    }
    fn serialize_none(self) -> Result<()> {
        self.tag(17)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<()> {
        self.tag(18)?;
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<()> {
        self.tag(19)
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<()> {
        self.tag(20)?;
        self.name(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.tag(21)?;
        self.name(name)?;
        self.name(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<()> {
        self.tag(22)?;
        self.name(name)?;
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.tag(23)?;
        self.name(name)?;
        self.name(variant)?;
        value.serialize(self)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.collection(24, len)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.collection(25, Some(len))
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.tag(26)?;
        self.name(name)?;
        Ok(self.sequence(Some(len)))
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.tag(27)?;
        self.name(name)?;
        self.name(variant)?;
        Ok(self.sequence(Some(len)))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        self.fields(28, len)
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.tag(29)?;
        self.name(name)?;
        Ok(self.dictionary(Some(len)))
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.tag(30)?;
        self.name(name)?;
        self.name(variant)?;
        Ok(self.dictionary(Some(len)))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}

pub struct Sequence<'a, W> {
    encoder: &'a mut Encoder<W>,
    expected: Option<usize>,
    count: usize,
}
impl<W: Write> Sequence<'_, W> {
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.encoder)?;
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Result<()> {
        check_length(self.expected, self.count)?;
        self.encoder.tag(END)
    }
}
macro_rules! sequence {
    ($($trait:ident::$method:ident),* $(,)?) => {$(
        impl<W: Write> ser::$trait for Sequence<'_, W> {
            type Ok = (); type Error = Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> { self.element(value) }
            fn end(self) -> Result<()> { self.finish() }
        }
    )*};
}
sequence! { SerializeSeq::serialize_element, SerializeTuple::serialize_element,
SerializeTupleStruct::serialize_field, SerializeTupleVariant::serialize_field }

pub struct Fields<'a, W> {
    encoder: &'a mut Encoder<W>,
    expected: Option<usize>,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    pending_key: Option<Vec<u8>>,
}
fn fragment<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value.serialize(&mut Encoder(&mut bytes))?;
    Ok(bytes)
}
fn check_length(expected: Option<usize>, count: usize) -> Result<()> {
    if expected.is_some_and(|len| len != count) {
        return Err(Error(
            "collection length differs from its declaration".into(),
        ));
    }
    Ok(())
}
impl<W: Write> Fields<'_, W> {
    fn field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<()> {
        self.entries.push((fragment(key)?, fragment(value)?));
        Ok(())
    }
    fn finish(mut self) -> Result<()> {
        if self.pending_key.is_some() {
            return Err(Error("map key has no value".into()));
        }
        check_length(self.expected, self.entries.len())?;
        self.entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        if self.entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(Error("duplicate encoded map key".into()));
        }
        for (key, value) in self.entries {
            self.encoder.0.write_all(&key)?;
            self.encoder.0.write_all(&value)?;
        }
        self.encoder.tag(END)
    }
}
impl<W: Write> ser::SerializeMap for Fields<'_, W> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        if self.pending_key.is_some() {
            return Err(Error("map value missing before next key".into()));
        }
        self.pending_key = Some(fragment(key)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| Error("map value has no key".into()))?;
        self.entries.push((key, fragment(value)?));
        Ok(())
    }
    fn end(self) -> Result<()> {
        self.finish()
    }
}
macro_rules! fields {
    ($($trait:ident),* $(,)?) => {$(
        impl<W: Write> ser::$trait for Fields<'_, W> {
            type Ok = (); type Error = Error;
            fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<()> { self.field(key, value) }
            fn end(self) -> Result<()> { self.finish() }
        }
    )*};
}
fields! { SerializeStruct, SerializeStructVariant }

#[cfg(test)]
mod tests;
