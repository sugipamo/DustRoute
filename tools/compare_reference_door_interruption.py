#!/usr/bin/env python3
"""Replay one measured door pulse; retain negative outcomes without adopting it.

Consumes observe_reference_door.py --interrupted artifacts. The existing full
tick/write/callback comparator remains the authority for model conformance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from observation_fixture import ROOT
from observation_records import digest, save


def compact(live, raw_path):
    assert live['schema_version'] == 'dustroute.reference-door-live.v1'
    assert live['interrupted_input_probe'] is True
    assert [i['powered'] for i in live['inputs']] == [True, False]
    assert live['inputs'][0]['tick'] == 0
    assert live['cleanup']['region_empty'] and live['cleanup']['force_load_removed']
    assert live['evidence']['contiguous_capture_window']
    assert live['evidence']['full_world_tick_heartbeats']
    assert live['evidence']['whole_region_readback_matches']
    worlds = {}
    ticks = []
    for sample in live['tick_end_states']:
        world = sample['blocks']
        key = hashlib.sha256(json.dumps(world, separators=(',', ':')).encode()).hexdigest()
        worlds[key] = world
        ticks.append(dict(tick=sample['tick'], world=key))
    return dict(
        schema_version='dustroute.reference-door-observation.v1',
        minecraft_version=live['minecraft_version'], fixture=live['fixture'],
        initial=live['initial'], inputs=live['inputs'], outcomes=live['outcomes'],
        world_states=worlds, tick_end_worlds=ticks, state_commits=live['state_commits'],
        cleanup=live['cleanup'], evidence=live['evidence'],
        source_capture_sha256=digest(raw_path),
        limit='One measured ON/OFF pulse, including incorrect aperture; no arbitrary-input, construction or adoption certificate.',
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--model-binary', type=Path,
                        default=ROOT / 'target/debug/examples/compare_electrical_pistons')
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    path = lambda suffix: prefix.with_suffix(suffix)
    destinations = [path(s) for s in ('.observed.json', '.input.json', '.model.json', '.comparison.json', '.callbacks.json')]
    assert not any(p.exists() for p in destinations), 'refusing to replace comparison evidence'
    live = json.loads(path('.reference.json').read_text())
    reference = compact(live, path('.raw.ndjson'))
    save(path('.observed.json'), reference)
    trial = dict(initial=reference['initial'],
                 inputs=[dict(i, after_world_tick=True) for i in reference['inputs']],
                 trace=True, verify_restoration=True)
    save(path('.input.json'), trial)
    with path('.model.json').open('x') as output:
        subprocess.run([str(args.model_binary.resolve()), str(path('.input.json'))],
                       stdout=output, check=True, timeout=180)
    result = subprocess.run([
        sys.executable, str(ROOT / 'tools/compare_reference_door.py'),
        '--reference', str(path('.observed.json')), '--input', str(path('.input.json')),
        '--model', str(path('.model.json')), '--raw', str(path('.raw.ndjson')),
        '--client', str(path('.client.json')), '--output', str(path('.comparison.json')),
        '--retain-callbacks', str(path('.callbacks.json')),
    ], check=False, timeout=180)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
