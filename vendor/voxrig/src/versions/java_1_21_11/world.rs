//! 1.21.11 chunk wire format and dimension-bound state storage.
use super::wire::{Reader, unpack_position};
use anyhow::{Context, Result, bail};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Dimension {
    pub min_y: i32,
    pub height: i32,
}
impl Dimension {
    pub fn new(min_y: i32, height: i32) -> Result<Self> {
        if min_y % 16 != 0
            || height % 16 != 0
            || !(16..=4096).contains(&height)
            || min_y.checked_add(height).is_none()
        {
            bail!("invalid dimension height");
        }
        Ok(Self { min_y, height })
    }
}

#[derive(Default)]
pub(crate) struct World {
    pub dimension: Option<(String, Dimension)>,
    chunks: HashMap<(i32, i32), Vec<Arc<[i32; 4096]>>>,
    pub revision: u64,
}

impl World {
    #[cfg(test)]
    pub(super) fn seed_replay_cell(&mut self, p: [i32; 3], id: i32) {
        let dimension = self.dimension.as_ref().unwrap().1;
        self.chunks
            .entry((p[0].div_euclid(16), p[2].div_euclid(16)))
            .or_insert_with(|| {
                let empty = Arc::new([0; 4096]);
                vec![empty; (dimension.height / 16) as usize]
            });
        self.set_block(p, id).unwrap();
    }
    pub fn select_dimension(&mut self, name: String, dimension: Dimension) {
        self.dimension = Some((name, dimension));
        self.chunks.clear();
        self.revision += 1;
    }
    pub fn reset(&mut self) {
        self.dimension = None;
        self.chunks.clear();
        self.revision += 1;
    }
    pub fn block(&self, p: [i32; 3]) -> Option<i32> {
        let dimension = self.dimension.as_ref()?.1;
        if p[1] < dimension.min_y || p[1] >= dimension.min_y + dimension.height {
            return None;
        }
        let sections = self
            .chunks
            .get(&(p[0].div_euclid(16), p[2].div_euclid(16)))?;
        let section = &sections[((p[1] - dimension.min_y) / 16) as usize];
        Some(
            section[(p[1].rem_euclid(16) * 256 + p[2].rem_euclid(16) * 16 + p[0].rem_euclid(16))
                as usize],
        )
    }
    fn set_block(&mut self, p: [i32; 3], id: i32) -> Result<()> {
        super::validate_state_id(id)?;
        let dimension = self
            .dimension
            .as_ref()
            .context("block update before dimension")?
            .1;
        if p[1] < dimension.min_y || p[1] >= dimension.min_y + dimension.height {
            bail!("block update outside dimension");
        }
        if let Some(sections) = self
            .chunks
            .get_mut(&(p[0].div_euclid(16), p[2].div_euclid(16)))
        {
            let section = &mut sections[((p[1] - dimension.min_y) / 16) as usize];
            Arc::make_mut(section)[(p[1].rem_euclid(16) * 256
                + p[2].rem_euclid(16) * 16
                + p[0].rem_euclid(16)) as usize] = id;
            self.revision += 1;
        }
        Ok(())
    }
    pub fn block_change(&mut self, payload: &[u8]) -> Result<Vec<([i32; 3], i32)>> {
        let mut r = Reader::new(payload);
        let p = unpack_position(r.u64()?);
        let id = r.varint()?;
        r.end()?;
        self.set_block(p, id)?;
        Ok(vec![(p, id)])
    }
    pub fn section_changes(&mut self, payload: &[u8]) -> Result<Vec<([i32; 3], i32)>> {
        let mut r = Reader::new(payload);
        let p = r.u64()?;
        let x = (p as i64 >> 42) as i32;
        let z = ((p << 22) as i64 >> 42) as i32;
        let y = ((p << 44) as i64 >> 44) as i32;
        let count = r.count(4096)?;
        let mut changes = Vec::with_capacity(count);
        for _ in 0..count {
            let record = r.varint()?;
            if record < 0 {
                bail!("negative block change");
            }
            let id = record >> 12;
            super::validate_state_id(id)?;
            changes.push((
                [
                    x * 16 + ((record >> 8) & 15),
                    y * 16 + (record & 15),
                    z * 16 + ((record >> 4) & 15),
                ],
                id,
            ));
        }
        r.end()?;
        let dimension = self
            .dimension
            .as_ref()
            .context("section update before dimension")?
            .1;
        if changes
            .iter()
            .any(|(p, _)| p[1] < dimension.min_y || p[1] >= dimension.min_y + dimension.height)
        {
            bail!("section update outside dimension");
        }
        for (p, id) in &changes {
            self.set_block(*p, *id)?;
        }
        Ok(changes)
    }
    pub fn unload(&mut self, payload: &[u8]) -> Result<()> {
        let mut r = Reader::new(payload);
        let z = r.i32()?;
        let x = r.i32()?;
        r.end()?;
        self.chunks.remove(&(x, z));
        self.revision += 1;
        Ok(())
    }
    pub fn loaded_chunks(&self) -> impl Iterator<Item = [i32; 2]> + '_ {
        self.chunks.keys().map(|&(x, z)| [x, z])
    }
    pub fn load(
        &mut self,
        payload: &[u8],
        maximum: usize,
    ) -> Result<Vec<([i32; 3], super::piston_nbt::PistonData)>> {
        let mut r = Reader::new(payload);
        let x = r.i32()?;
        let z = r.i32()?;
        if !(-1_875_000..=1_875_000).contains(&x) || !(-1_875_000..=1_875_000).contains(&z) {
            bail!("chunk outside border");
        }
        if !self.chunks.contains_key(&(x, z)) && self.chunks.len() >= maximum {
            bail!("chunk cache limit exceeded");
        }
        let dimension = self.dimension.as_ref().context("chunk before dimension")?.1;
        for _ in 0..r.count(16)? {
            r.varint()?;
            let count = r.count(1024)?;
            r.take(count * 8)?;
        }
        let mut data = Reader::new(r.byte_array(2_097_152)?);
        let mut sections: Vec<Arc<[i32; 4096]>> = Vec::new();
        for _ in 0..dimension.height / 16 {
            let non_air = data.u16()?;
            if non_air > 4096 {
                bail!("invalid non-air count");
            }
            let states = palette(&mut data, 4096, 4, 8, 16)?;
            for id in &states {
                super::validate_state_id(*id)?;
            }
            sections.push(Arc::new(
                states
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("invalid section volume"))?,
            ));
            palette(&mut data, 64, 1, 3, 8)?;
        }
        data.end()?;
        let mut pistons = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..r.count(65_536)? {
            let xz = r.u8()?;
            let y = r.u16()? as i16 as i32;
            let p = [x * 16 + i32::from(xz >> 4), y, z * 16 + i32::from(xz & 15)];
            if y < dimension.min_y || y >= dimension.min_y + dimension.height || !seen.insert(p) {
                bail!("invalid/duplicate block entity position");
            }
            let kind = r.varint()?;
            if kind < 0 {
                bail!("negative block entity type");
            }
            if kind == super::piston_nbt::PISTON_TYPE {
                let id = sections[((y - dimension.min_y) / 16) as usize][(y.rem_euclid(16) * 256
                    + p[2].rem_euclid(16) * 16
                    + p[0].rem_euclid(16))
                    as usize];
                let carrier = super::native_state(id)?;
                if carrier.name != "minecraft:moving_piston" {
                    bail!("piston entity without moving state");
                }
                if let Some(data) = super::piston_nbt::PistonData::read(&mut r)? {
                    if carrier.properties.get("facing").map(String::as_str)
                        != Some(data.direction.name())
                    {
                        bail!("piston entity facing mismatch");
                    }
                    if pistons.len() >= 4096 {
                        bail!("chunk moving carrier limit exceeded");
                    }
                    pistons.push((p, data));
                }
            } else {
                r.skip_optional_nbt()?;
            }
        }
        for _ in 0..4 {
            let count = r.count(1024)?;
            r.take(count * 8)?;
        }
        for _ in 0..2 {
            for _ in 0..r.count(1024)? {
                if r.byte_array(2048)?.len() != 2048 {
                    bail!("invalid light array size");
                }
            }
        }
        r.end()?;
        self.chunks.insert((x, z), sections);
        self.revision += 1;
        Ok(pistons)
    }
}

fn palette(
    r: &mut Reader<'_>,
    count: usize,
    min_bits: u8,
    indirect_max: u8,
    max_bits: u8,
) -> Result<Vec<i32>> {
    let bits = r.u8()?;
    if bits == 0 {
        let value = r.varint()?;
        if value < 0 {
            bail!("negative palette ID");
        }
        return Ok(vec![value; count]);
    }
    if bits < min_bits || bits > max_bits {
        bail!("invalid palette width {bits}");
    }
    let palette = if bits <= indirect_max {
        let size = r.count(1usize << bits)?;
        if size == 0 {
            bail!("empty palette");
        }
        let mut palette = Vec::with_capacity(size);
        for _ in 0..size {
            let id = r.varint()?;
            if id < 0 {
                bail!("negative palette ID");
            }
            palette.push(id);
        }
        Some(palette)
    } else {
        None
    };
    let per_long = 64 / bits as usize;
    let mask = (1u64 << bits) - 1;
    let mut output = Vec::with_capacity(count);
    for _ in 0..count.div_ceil(per_long) {
        let word = r.u64()?;
        for index in 0..per_long {
            if output.len() == count {
                break;
            }
            let value = ((word >> (index * bits as usize)) & mask) as i32;
            output.push(if let Some(palette) = &palette {
                *palette
                    .get(value as usize)
                    .context("palette index out of range")?
            } else {
                value
            });
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palette_uses_no_length_prefix_and_rejects_bad_indices() {
        assert_eq!(
            palette(&mut Reader::new(&[0, 1]), 4096, 4, 8, 16).unwrap(),
            vec![1; 4096]
        );
        let mut bytes = vec![4, 2, 0, 1];
        bytes.extend(0x10u64.to_be_bytes());
        assert_eq!(
            palette(&mut Reader::new(&bytes), 2, 4, 8, 16).unwrap(),
            vec![0, 1]
        );
        let mut invalid = vec![4, 1, 0];
        invalid.extend(1u64.to_be_bytes());
        assert!(palette(&mut Reader::new(&invalid), 1, 4, 8, 16).is_err());
        assert!(palette(&mut Reader::new(&[32]), 4096, 4, 8, 16).is_err());
        assert!(palette(&mut Reader::new(&[4, 1, 0]), 4096, 4, 8, 16).is_err());
    }
    #[test]
    fn negative_height_updates_unload_and_dimension_reset_are_distinct() {
        let mut world = World::default();
        world.select_dimension(
            "minecraft:overworld".into(),
            Dimension::new(-64, 384).unwrap(),
        );
        assert_eq!(world.block([0, -64, 0]), None);
        let empty = Arc::new([0; 4096]);
        world.chunks.insert((0, 0), vec![empty; 24]);
        world.set_block([0, -64, 0], 1).unwrap();
        assert_eq!(world.block([0, -64, 0]), Some(1));
        assert_eq!(world.block([0, 319, 0]), Some(0));
        assert_eq!(world.block([0, 320, 0]), None);
        assert!(world.set_block([0, -65, 0], 1).is_err());
        world.unload(&[0; 8]).unwrap();
        assert_eq!(world.block([0, -64, 0]), None);
        world.reset();
        assert!(world.dimension.is_none());
    }
}
