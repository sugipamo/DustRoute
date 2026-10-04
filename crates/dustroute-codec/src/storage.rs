//! Versioned historical data, never a live operation or authorization codec.
//! Callers select a record schema and byte bound, and validate domain invariants
//! after decoding. Filesystem locks, ownership and durability stay with callers.
mod decoder;
use serde::{Serialize, de::DeserializeOwned};
use std::{fmt, io::Write};

const HEADER: &[u8] = b"dustroute.storage.v1\0";
const PAYLOAD_HEADER: &[u8] = b"dustroute.canonical-key.v1\0";

#[derive(Debug)]
pub enum Error {
    ByteLimit,
    Schema,
    Invalid(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ByteLimit => f.write_str("stored record exceeds byte bound"),
            Self::Schema => f.write_str("unsupported stored record schema"),
            Self::Invalid(message) => message.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl serde::de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Invalid(message.to_string())
    }
}
type Result<T> = std::result::Result<T, Error>;

/// Bounds both encoded output and the serializer's final file payload. The
/// encoder still buffers individual map entries; callers bound their own model.
pub fn encode(
    schema: &str,
    value: &(impl Serialize + ?Sized),
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter {
        bytes: Vec::new(),
        max_bytes,
        exceeded: false,
    };
    let result = (|| {
        writer.write_all(HEADER).map_err(io_error)?;
        writer
            .write_all(&(schema.len() as u64).to_be_bytes())
            .map_err(io_error)?;
        writer.write_all(schema.as_bytes()).map_err(io_error)?;
        crate::canonical::write(&mut writer, value).map_err(|e| Error::Invalid(e.to_string()))
    })();
    if writer.exceeded {
        return Err(Error::ByteLimit);
    }
    result?;
    // Check format limits before callers durably replace an existing record.
    // This does not validate a domain claim or grant any operation authority.
    decode::<serde::de::IgnoredAny>(schema, &writer.bytes, max_bytes)?;
    Ok(writer.bytes)
}

/// No compatibility fallback: old JSON, other record kinds, trailing data and
/// excessive nesting/entry counts are refused before returning historical data.
pub fn decode<T: DeserializeOwned>(schema: &str, bytes: &[u8], max_bytes: usize) -> Result<T> {
    if bytes.len() > max_bytes {
        return Err(Error::ByteLimit);
    }
    let mut decoder = decoder::Decoder::new(bytes);
    decoder.expect_bytes(HEADER)?;
    if decoder.text_payload()? != schema {
        return Err(Error::Schema);
    }
    decoder.expect_bytes(PAYLOAD_HEADER)?;
    let value = T::deserialize(&mut decoder)?;
    decoder.finish()?;
    Ok(value)
}

fn io_error(e: std::io::Error) -> Error {
    Error::Invalid(e.to_string())
}
struct BoundedWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    exceeded: bool,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.max_bytes.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("stored record exceeds byte bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
