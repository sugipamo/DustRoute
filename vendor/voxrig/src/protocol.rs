use anyhow::{Context, Result, bail};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_PACKET_SIZE: usize = 2_097_152;

pub fn put_varint(out: &mut Vec<u8>, mut value: i32) {
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            return;
        }
        out.push(((value & 0x7f) | 0x80) as u8);
        value = ((value as u32) >> 7) as i32;
    }
}

pub fn get_varint(input: &mut &[u8]) -> Result<i32> {
    let mut value = 0u32;
    for position in 0..5 {
        let byte = *input.first().context("unexpected EOF in VarInt")?;
        *input = &input[1..];
        value |= u32::from(byte & 0x7f) << (position * 7);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    bail!("VarInt exceeds five bytes")
}

pub async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> Result<i32> {
    let mut bytes = Vec::with_capacity(5);
    loop {
        let byte = reader.read_u8().await?;
        bytes.push(byte);
        if byte & 0x80 == 0 {
            break;
        }
        if bytes.len() == 5 {
            bail!("VarInt exceeds five bytes");
        }
    }
    get_varint(&mut bytes.as_slice())
}

pub fn put_string(out: &mut Vec<u8>, value: &str) {
    put_varint(out, value.len() as i32);
    out.extend_from_slice(value.as_bytes());
}

pub fn get_string(input: &mut &[u8]) -> Result<String> {
    let len = get_varint(input)?;
    const MAX_STRING_BYTES: usize = 32_767 * 4;
    if len < 0 || len as usize > MAX_STRING_BYTES || input.len() < len as usize {
        bail!("invalid string length {len}");
    }
    let value = std::str::from_utf8(&input[..len as usize])?.to_owned();
    if value.chars().count() > 32_767 {
        bail!("string exceeds 32767 characters");
    }
    *input = &input[len as usize..];
    Ok(value)
}

pub async fn read_packet<R: AsyncRead + Unpin>(
    reader: &mut R,
    compression: Option<i32>,
) -> Result<(i32, Vec<u8>)> {
    let frame_len = read_varint(reader).await?;
    if !(0..=MAX_PACKET_SIZE as i32).contains(&frame_len) {
        bail!("invalid packet length {frame_len}");
    }
    let mut frame = vec![0; frame_len as usize];
    reader.read_exact(&mut frame).await?;
    let body = if let Some(threshold) = compression {
        let mut cursor = frame.as_slice();
        let uncompressed_len = get_varint(&mut cursor)?;
        if uncompressed_len == 0 {
            if threshold >= 0 && cursor.len() >= threshold as usize {
                bail!("uncompressed packet meets compression threshold");
            }
            cursor.to_vec()
        } else {
            if !(1..=MAX_PACKET_SIZE as i32).contains(&uncompressed_len) {
                bail!("invalid uncompressed packet length {uncompressed_len}");
            }
            if threshold >= 0 && uncompressed_len < threshold {
                bail!("compressed packet is below compression threshold");
            }
            let mut decoder = ZlibDecoder::new(cursor);
            let mut decoded = Vec::with_capacity(uncompressed_len as usize);
            decoder
                .by_ref()
                .take(uncompressed_len as u64 + 1)
                .read_to_end(&mut decoded)?;
            if decoded.len() != uncompressed_len as usize {
                bail!("decompressed length mismatch");
            }
            decoded
        }
    } else {
        frame
    };
    let mut cursor = body.as_slice();
    let id = get_varint(&mut cursor)?;
    Ok((id, cursor.to_vec()))
}

pub async fn write_packet<W: AsyncWrite + Unpin>(
    writer: &mut W,
    compression: Option<i32>,
    id: i32,
    payload: &[u8],
) -> Result<()> {
    let mut body = Vec::new();
    put_varint(&mut body, id);
    body.extend_from_slice(payload);
    if body.len() > MAX_PACKET_SIZE {
        bail!("packet body exceeds {MAX_PACKET_SIZE} bytes");
    }
    let frame = match compression {
        Some(threshold) if body.len() >= threshold.max(0) as usize => {
            let mut compressed = ZlibEncoder::new(Vec::new(), Compression::default());
            compressed.write_all(&body)?;
            let compressed = compressed.finish()?;
            let mut frame = Vec::new();
            put_varint(&mut frame, body.len() as i32);
            frame.extend(compressed);
            frame
        }
        Some(_) => {
            let mut frame = Vec::new();
            put_varint(&mut frame, 0);
            frame.extend(body);
            frame
        }
        None => body,
    };
    if frame.len() > MAX_PACKET_SIZE {
        bail!("packet frame exceeds {MAX_PACKET_SIZE} bytes");
    }
    let mut prefix = Vec::new();
    put_varint(&mut prefix, frame.len() as i32);
    writer.write_all(&prefix).await?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn varint_examples() {
        for value in [0, 1, 127, 128, 255, 2_147_483_647, -1] {
            let mut encoded = Vec::new();
            put_varint(&mut encoded, value);
            assert_eq!(get_varint(&mut encoded.as_slice()).unwrap(), value);
        }
    }

    #[tokio::test]
    async fn packet_round_trip_with_each_compression_mode() {
        for compression in [None, Some(0), Some(256)] {
            let (mut tx, mut rx) = tokio::io::duplex(4096);
            let payload = b"structured sound observation";
            write_packet(&mut tx, compression, 0x51, payload)
                .await
                .unwrap();
            let (id, decoded) = read_packet(&mut rx, compression).await.unwrap();
            assert_eq!(id, 0x51);
            assert_eq!(decoded, payload);
        }
    }

    #[tokio::test]
    async fn compressed_packet_rejects_negative_declared_length() {
        let mut frame = Vec::new();
        put_varint(&mut frame, -1);
        let mut wire = Vec::new();
        put_varint(&mut wire, frame.len() as i32);
        wire.extend(frame);
        assert!(read_packet(&mut wire.as_slice(), Some(0)).await.is_err());
    }

    #[tokio::test]
    async fn compressed_packet_rejects_output_larger_than_declared() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&[0, 1, 2]).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut frame = Vec::new();
        put_varint(&mut frame, 2);
        frame.extend(compressed);
        let mut wire = Vec::new();
        put_varint(&mut wire, frame.len() as i32);
        wire.extend(frame);
        assert!(read_packet(&mut wire.as_slice(), Some(0)).await.is_err());
    }
}
