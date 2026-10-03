"""Saved chunk evidence must keep stale snapshots distinct from live queues."""
import hashlib
from pathlib import Path
import struct
import tempfile
import unittest
import zlib

from read_clock_block_ticks import NbtReader, read_ticks


def text(value):
    value = value.encode()
    return struct.pack('>H', len(value)) + value


def field(tag, name, data):
    return bytes([tag]) + text(name) + data


class SavedQueueEvidence(unittest.TestCase):
    def test_negative_chunk_coordinates_filter_and_stale_time(self):
        target = dict(x=-17, y=180, z=-1)
        with tempfile.TemporaryDirectory() as directory:
            region = Path(directory) / 'world/region/r.-1.-1.mca'
            region.parent.mkdir(parents=True)
            entries = b''
            for x, delay in [(-17, 8), (-18, 99)]:
                entries += field(8, 'i', text('minecraft:redstone_torch'))
                entries += b''.join(field(3, k, struct.pack('>i', v))
                                    for k, v in dict(x=x, y=180, z=-1, t=delay, p=-1).items()) + b'\0'
            body = field(3, 'xPos', struct.pack('>i', -2))
            body += field(3, 'zPos', struct.pack('>i', -1))
            body += field(4, 'LastUpdate', struct.pack('>q', 200))
            body += field(9, 'block_ticks', bytes([10]) + struct.pack('>i', 2) + entries)
            payload = zlib.compress(b'\x0a\0\0' + body + b'\0')
            data = bytearray(3 * 4096)
            index = 4 * (30 + 32 * 31)
            data[index:index + 4] = b'\0\0\x02\x01'
            data[8192:8197 + len(payload)] = struct.pack('>I', len(payload) + 1) + b'\x02' + payload
            region.write_bytes(data)
            current = read_ticks(directory, **target, current_time=200)
            self.assertTrue(current['current_queue_verified'])
            self.assertEqual(current['region_sha256'], hashlib.sha256(data).hexdigest())
            self.assertEqual(current['ticks'], [dict(block='minecraft:redstone_torch', stored_delay=8, priority=-1)])
            stale = read_ticks(directory, **target, current_time=220)
            self.assertFalse(stale['current_queue_verified'])
            self.assertEqual(stale['saved_chunk_age_game_ticks'], 20)
            self.assertEqual(stale['ticks'][0]['stored_delay'], 8)
            with self.assertRaises(ValueError):
                read_ticks(directory, **target, current_time=199)
            with self.assertRaises(ValueError):
                read_ticks(directory, x=-16, y=180, z=-1, current_time=200)

    def test_truncated_negative_length_and_trailing_nbt_are_rejected(self):
        for payload in [b'', b'\x0a\0\0', b'\x0a\0\0\0\0',
                        b'\x0a\0\0' + field(9, 'ticks', b'\x0a\xff\xff\xff\xff')]:
            with self.subTest(payload=payload), self.assertRaises(ValueError):
                NbtReader(payload).root()


if __name__ == '__main__':
    unittest.main()
