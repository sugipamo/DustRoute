#!/usr/bin/env python3
"""Retain an independently re-derived, bounded device-circuit observation.

The gzip file is a lossless JSON container: exact raw header/window/end,
reviewed fixture, required client observations and derived expectations. It is
not the full original capture; hashes identify the unmodified original files.
"""
import argparse
import gzip
import json
from pathlib import Path

from compare_device_circuits import observe
from observation_records import digest


def retain(prefix, fixture_path):
    raw_path, client_path = prefix.with_suffix('.raw.ndjson'), prefix.with_suffix('.client.json')
    raw = [json.loads(line) for line in raw_path.read_text().splitlines()]
    fixture = json.loads(fixture_path.read_text())
    client = json.loads(client_path.read_text())
    observation = observe(raw, client, fixture)
    first, last = observation['applied_inputs'][0], observation['applied_inputs'][-1]
    begin = max(i for i, r in enumerate(raw) if r.get('sequence', float('inf')) < first['packet_sequence']
                and r['kind'] == 'server_world_tick' and r.get('dimension') == 'minecraft:overworld'
                and r['phase'] == 'end' and r['game_tick'] == first['game_tick'])
    stop = last['game_tick'] + fixture.get('compare_drain_ticks', 40)
    end = next(i for i, r in enumerate(raw) if i > begin and r['kind'] == 'server_world_tick'
               and r.get('dimension') == 'minecraft:overworld' and r['phase'] == 'end' and r['game_tick'] == stop)
    retained_raw = [raw[0], *raw[begin:end + 1], raw[-1]]
    client_keys = ['complete', 'cleanup', 'enabled_features', 'known_region', 'origin', 'activations', 'initial', 'final']
    retained_client = {k: client[k] for k in client_keys}
    if 'server_readbacks' in client:
        retained_client['server_readbacks'] = client['server_readbacks']
    # Verify the retained interval can still independently derive every expected
    # value. No model file is read while promoting an observation.
    assert observe(retained_raw, retained_client, fixture) == observation
    return dict(schema_version='dustroute.retained-device-circuit.v1',
                fixture=fixture, client=retained_client, raw_interval=retained_raw,
                observed=observation,
                sources={str(p): digest(p) for p in [raw_path, client_path, fixture_path]},
                raw_interval_is_full_artifact=False,
                scope='declared contiguous input/drain window with original header/end; not arbitrary live-state reconstruction')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    record = retain(args.prefix, args.fixture)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    encoded = json.dumps(record, separators=(',', ':')).encode()
    with args.output.open('xb') as out:
        out.write(gzip.compress(encoded, mtime=0))
    print(json.dumps(dict(output=str(args.output), uncompressed_bytes=len(encoded), sha256=digest(args.output))))


if __name__ == '__main__':
    main()
