#!/usr/bin/env python3
"""Read saved chunk tick evidence, independently of a Minecraft client.

A stored delay is relative to LastUpdate. A stale saved chunk is explicitly
reported; it never becomes evidence of the current server queue.
"""
import hashlib
from pathlib import Path
import struct
import zlib

LIMIT = 16 * 1024 * 1024


class NbtReader:
    def __init__(self, data):
        self.data, self.offset, self.nodes = data, 0, 0

    def take(self, length):
        if not 0 <= length <= len(self.data) - self.offset:
            raise ValueError('truncated or oversized NBT value')
        start = self.offset
        self.offset += length
        return self.data[start:self.offset]

    def number(self, form):
        return struct.unpack('>' + form, self.take(struct.calcsize(form)))[0]

    def string(self):
        # NBT uses Java modified UTF-8; preserve surrogate code units in fields
        # outside the ASCII keys used by this evidence reader.
        return self.take(self.number('H')).replace(b'\xc0\x80', b'\0').decode('utf-8', 'surrogatepass')

    def count(self):
        count = self.number('i')
        if not 0 <= count <= LIMIT:
            raise ValueError('invalid NBT collection length')
        return count

    def value(self, tag, depth=0):
        self.nodes += 1
        if depth > 128 or self.nodes > 1_000_000:
            raise ValueError('NBT nesting or node limit exceeded')
        if tag in (1, 2, 3, 4, 5, 6):
            return self.number({1: 'b', 2: 'h', 3: 'i', 4: 'q', 5: 'f', 6: 'd'}[tag])
        if tag == 7:
            return self.take(self.count())
        if tag == 8:
            return self.string()
        if tag == 9:
            kind, count = self.number('B'), self.count()
            if count and kind == 0:
                raise ValueError('nonempty TAG_End list')
            return [self.value(kind, depth + 1) for _ in range(count)]
        if tag == 10:
            fields = {}
            while (kind := self.number('B')) != 0:
                name = self.string()
                if name in fields:
                    raise ValueError('duplicate NBT compound key')
                fields[name] = self.value(kind, depth + 1)
            return fields
        if tag in (11, 12):
            return [self.number('i' if tag == 11 else 'q') for _ in range(self.count())]
        raise ValueError('unknown NBT tag')

    def root(self):
        if self.number('B') != 10:
            raise ValueError('chunk root must be a compound')
        self.string()
        result = self.value(10)
        if self.offset != len(self.data):
            raise ValueError('trailing NBT data')
        return result


def read_ticks(directory, x, y, z, current_time):
    cx, cz = x // 16, z // 16
    path = Path(directory) / 'world' / 'region' / f'r.{cx // 32}.{cz // 32}.mca'
    with path.open('rb') as stream:
        stream.seek(4 * (cx % 32 + 32 * (cz % 32)))
        entry = stream.read(4)
        if len(entry) != 4:
            raise ValueError('truncated region header')
        sector, count = int.from_bytes(entry[:3], 'big'), entry[3]
        if sector < 2 or count == 0:
            raise ValueError('chunk is not stored')
        stream.seek(sector * 4096)
        header = stream.read(5)
        if len(header) != 5:
            raise ValueError('truncated chunk header')
        length = int.from_bytes(header[:4], 'big')
        if not 1 < length <= count * 4096 - 4 or header[4] != 2:
            raise ValueError('expected inline zlib-compressed chunk')
        packed = stream.read(length - 1)
        if len(packed) != length - 1:
            raise ValueError('truncated chunk payload')
        stream.seek(0)
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    decoder = zlib.decompressobj()
    data = decoder.decompress(packed, LIMIT + 1)
    if len(data) > LIMIT or not decoder.eof or decoder.unused_data:
        raise ValueError('oversized or incomplete compressed chunk')
    chunk = NbtReader(data).root()
    if (chunk['xPos'], chunk['zPos']) != (cx, cz):
        raise ValueError('chunk coordinates differ')
    age = current_time - chunk['LastUpdate']
    if not 0 <= age <= (1 << 53) - 1:
        raise ValueError('saved chunk age is outside the evidence range')
    ticks = chunk['block_ticks']
    if not isinstance(ticks, list):
        raise ValueError('block_ticks must be a list')
    return dict(evidence='saved_server_chunk_block_ticks',
                saved_chunk_age_game_ticks=age, current_queue_verified=age == 0,
                region_sha256=digest,
                ticks=[dict(block=t['i'], stored_delay=t['t'], priority=t['p'])
                       for t in ticks if (t['x'], t['y'], t['z']) == (x, y, z)])
