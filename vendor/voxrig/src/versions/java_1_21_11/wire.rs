//! Bounded field decoding for the 1.21.11 adapter.
use anyhow::{Context, Result, bail};

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        if length > self.bytes.len() {
            bail!("truncated packet field");
        }
        let (field, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(field)
    }
    pub fn end(&self) -> Result<()> {
        if !self.bytes.is_empty() {
            bail!(
                "unexpected trailing packet data: {} bytes",
                self.bytes.len()
            );
        }
        Ok(())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => bail!("invalid packet boolean"),
        }
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        let value = f32::from_be_bytes(self.take(4)?.try_into()?);
        if !value.is_finite() {
            bail!("non-finite packet float");
        }
        Ok(value)
    }
    pub fn f64(&mut self) -> Result<f64> {
        let value = f64::from_be_bytes(self.take(8)?.try_into()?);
        if !value.is_finite() {
            bail!("non-finite packet double");
        }
        Ok(value)
    }
    pub fn varint(&mut self) -> Result<i32> {
        crate::protocol::get_varint(&mut self.bytes)
    }
    pub fn count(&mut self, maximum: usize) -> Result<usize> {
        let count = usize::try_from(self.varint()?).context("negative packet count")?;
        if count > maximum {
            bail!("packet count exceeds limit {maximum}");
        }
        Ok(count)
    }
    pub fn string(&mut self) -> Result<String> {
        crate::protocol::get_string(&mut self.bytes)
    }
    pub fn byte_array(&mut self, maximum: usize) -> Result<&'a [u8]> {
        let length = self.count(maximum)?;
        self.take(length)
    }
    pub(super) fn nbt_string(&mut self) -> Result<String> {
        let length = self.u16()? as usize;
        Ok(std::str::from_utf8(self.take(length)?)?.to_owned())
    }
    pub(super) fn nbt_field(&mut self) -> Result<Option<(u8, String)>> {
        let kind = self.u8()?;
        if kind == 0 {
            return Ok(None);
        }
        Ok(Some((kind, self.nbt_string()?)))
    }
    pub(super) fn skip_optional_nbt(&mut self) -> Result<()> {
        let kind = self.u8()?;
        if kind != 0 {
            self.skip_nbt_value(kind, 0)?;
        }
        Ok(())
    }
    /// Reads a whole unnamed compound, retaining only dimension height fields.
    pub fn dimension_nbt(&mut self) -> Result<(Option<i32>, Option<i32>)> {
        if self.u8()? != 10 {
            bail!("dimension registry entry must be a compound");
        }
        let (mut min_y, mut height) = (None, None);
        let mut count = 0;
        loop {
            let kind = self.u8()?;
            if kind == 0 {
                break;
            }
            count += 1;
            if count > 65_536 {
                bail!("NBT compound entry limit exceeded");
            }
            let name = self.nbt_string()?;
            match (name.as_str(), kind) {
                ("min_y", 3) => min_y = Some(self.i32()?),
                ("height", 3) => height = Some(self.i32()?),
                _ => self.skip_nbt_value(kind, 1)?,
            }
        }
        Ok((min_y, height))
    }
    pub fn skip_nbt(&mut self) -> Result<()> {
        let kind = self.u8()?;
        if kind == 0 {
            bail!("unexpected End as network NBT root");
        }
        self.skip_nbt_value(kind, 0)
    }
    pub(super) fn skip_nbt_value(&mut self, kind: u8, depth: usize) -> Result<()> {
        if depth > 64 {
            bail!("NBT depth limit exceeded");
        }
        match kind {
            1 => {
                self.take(1)?;
            }
            2 => {
                self.take(2)?;
            }
            3 | 5 => {
                self.take(4)?;
            }
            4 | 6 => {
                self.take(8)?;
            }
            7 | 11 | 12 => {
                let count = usize::try_from(self.i32()?).context("negative NBT array count")?;
                let width = match kind {
                    7 => 1,
                    11 => 4,
                    _ => 8,
                };
                self.take(
                    count
                        .checked_mul(width)
                        .context("NBT array length overflow")?,
                )?;
            }
            8 => {
                let length = self.u16()? as usize;
                self.take(length)?;
            }
            9 => {
                let item = self.u8()?;
                let count = usize::try_from(self.i32()?).context("negative NBT list count")?;
                if count > 65_536 || (item == 0 && count != 0) {
                    bail!("invalid NBT list count");
                }
                for _ in 0..count {
                    self.skip_nbt_value(item, depth + 1)?;
                }
            }
            10 => {
                let mut count = 0;
                loop {
                    let item = self.u8()?;
                    if item == 0 {
                        break;
                    }
                    count += 1;
                    if count > 65_536 {
                        bail!("NBT compound entry limit exceeded");
                    }
                    let length = self.u16()? as usize;
                    self.take(length)?;
                    self.skip_nbt_value(item, depth + 1)?;
                }
            }
            _ => bail!("invalid NBT tag {kind}"),
        }
        Ok(())
    }
}

pub(crate) fn unpack_position(p: u64) -> [i32; 3] {
    [
        (p as i64 >> 38) as i32,
        ((p << 52) as i64 >> 52) as i32,
        ((p << 26) as i64 >> 38) as i32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_negative_counts_truncation_non_finite_and_nbt_bombs() {
        assert!(Reader::new(&[255, 255, 255, 255, 15]).count(5).is_err());
        assert!(Reader::new(&[0, 1]).i32().is_err());
        assert!(Reader::new(&f64::NAN.to_be_bytes()).f64().is_err());
        assert!(Reader::new(&[7, 255, 255, 255, 255]).skip_nbt().is_err());
        assert!(Reader::new(&[9, 0, 0, 0, 0, 1]).skip_nbt().is_err());
        let mut deep = vec![10];
        for _ in 0..66 {
            deep.extend([10, 0, 0]);
        }
        assert!(Reader::new(&deep).skip_nbt().is_err());
    }
}
