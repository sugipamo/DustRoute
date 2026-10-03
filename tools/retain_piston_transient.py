#!/usr/bin/env python3
"""Retain server-derived transient expectations after verifying capture coverage."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from compare_piston_transients import compare, live_trace, model_trace
from observation_fixture import snapshot


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def retain(prefix, output):
    paths = {suffix: prefix.with_suffix(suffix) for suffix in
             ('.raw.ndjson', '.client.json', '.model.json', '.summary.json')}
    raw = [json.loads(line) for line in paths['.raw.ndjson'].read_text().splitlines()]
    client = json.loads(paths['.client.json'].read_text())
    summary = json.loads(paths['.summary.json'].read_text())
    assert summary['classification'] == 'matched_final_state'
    for suffix in ('.raw.ndjson', '.client.json', '.model.json'):
        assert summary['sha256'][suffix] == sha256(paths[suffix]), 'artifact changed after capture'
    observed, applied = live_trace(raw, client)
    initial = snapshot(client, 'initial')
    first = applied[0]['game_tick']
    inputs = [{'tick': row['game_tick'] - first + 1,
               'position': row['position'], 'powered': row['powered'], 'after_world_tick': True} for row in applied]
    model_binary = Path(__file__).resolve().parents[1] / 'target/debug/examples/compare_electrical_pistons'
    with tempfile.TemporaryDirectory() as directory:
        request = Path(directory) / 'input.json'
        request.write_text(json.dumps(dict(initial=initial, inputs=inputs, trace=True, verify_restoration=True)))
        replay = subprocess.run([str(model_binary),str(request)],capture_output=True,text=True,check=True,timeout=60)
        verified = json.loads(replay.stdout)
    assert verified['restoration_verified']
    predicted = model_trace(initial, verified['trace'])
    difference = compare(observed, predicted)
    stop = applied[-1]['game_tick'] + 8
    first_input_index = next(i for i,row in enumerate(raw) if row.get('sequence') == applied[0]['packet_sequence'])
    start_index = max(i for i,row in enumerate(raw[:first_input_index]) if row['kind']=='server_world_tick'
                      and row.get('phase')=='end' and row.get('dimension')=='minecraft:overworld')
    end_index = next(i for i, row in enumerate(raw) if i >= start_index
                     and row['kind'] == 'server_world_tick' and row.get('phase') == 'end'
                     and row.get('dimension') == 'minecraft:overworld' and row['game_tick'] == stop)
    compact = {
        'schema_version': 'dustroute.piston-transient-observation.v2',
        'run_id': prefix.name,
        'minecraft_version': '1.21.11',
        'classification_after_measured_boundary_replay': 'matched_transient_projection' if difference is None else 'transient_mismatch',
        'evidence_boundary': 'event/carrier boundaries and ordinary callbacks to active components; whole known block state; not every setter or shape callback',
        'initial': initial,
        'inputs': inputs,
        'observed_final': snapshot(client, 'final'),
        'observed_events': observed,
        'applied_inputs': applied,
        'cleanup': client['cleanup'],
        'raw_window': [raw[0], *raw[start_index:end_index + 1], raw[-1]],
        'provenance': {
            'artifacts': {suffix: str(path) for suffix, path in paths.items()},
            'sha256': {suffix: sha256(path) for suffix, path in paths.items()},
            'actor_sha256': summary['actor_sha256'],
            'model_exporter_sha256': summary['comparator_sha256'],
            'fixture_sha256': summary['fixture_sha256'],
            'replay_binary_sha256': sha256(model_binary),
            'replay_output_sha256': hashlib.sha256(replay.stdout.encode()).hexdigest(),
            'projection_sha256': sha256(Path(__file__).with_name('compare_piston_transients.py')),
        },
    }
    with output.open('x') as stream:
        json.dump(compact, stream, indent=2)
        stream.write('\n')
    print(json.dumps({'output': str(output), 'events': len(observed),
                      'classification': compact['classification_after_measured_boundary_replay']}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    retain(args.prefix, args.output)


if __name__ == '__main__':
    main()
