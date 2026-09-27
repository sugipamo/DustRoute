#!/usr/bin/env python3
"""Validate and copy a complete local torch observation into a fresh fixture."""
import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True
from observe_torch_burnout import schedules


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--reason', required=True)
    args = parser.parse_args()
    source = json.loads(args.source.read_text())
    raw = args.source.with_suffix('.console.log')
    assert source['schema'] == 'dustroute.torch-observation.v1'
    assert source['minecraft_version'] == '1.21.11'
    assert source['server_jar_sha1'] == '64bb6d763bed0a9f1d632ec347938594144943ed'
    assert source['evidence'] == 'observed_server_block_state' and source['complete'] is True
    assert source['clock'] == 'game_tick'
    assert source['sampling'] == 'frozen_console_after_each_explicit_tick'
    assert source['input_delivery'] == 'set_lever_state_then_glass_place_remove_neighbor_notification_in_same_frozen_tick'
    assert source['internal_callback_order'] == 'unavailable'
    assert hashlib.sha256(raw.read_bytes()).hexdigest() == source['raw_console_sha256']
    assert 'All dimensions are saved' in raw.read_text(), 'missing clean server stop'
    expected = {(name, orientation): (changes, duration) for name, changes, duration in schedules()
                for orientation in ['standing', 'wall']}
    actual = [(case['name'], case['orientation']) for case in source['cases']]
    assert len(actual) == len(set(actual)) and set(actual) == set(expected)
    for case in source['cases']:
        changes, duration = expected[(case['name'], case['orientation'])]
        assert case['duration_game_ticks'] == duration
        assert case['inputs'] == [{'game_tick': t, 'powered': v} for t, v in changes]
        assert len(case['samples']) == duration + 1
        for tick, sample in enumerate(case['samples']):
            assert sample['game_tick'] == tick and type(sample['lit']) is bool
    meta = args.destination.with_suffix('.meta.json')
    assert not args.destination.exists() and not meta.exists(), 'existing evidence is immutable'
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    # Keep the full normalized samples. No values are inferred from the model.
    args.destination.write_bytes(args.source.read_bytes())
    metadata = {
        'reason': args.reason,
        'source_artifact': args.source.name,
        'fixture_sha256': hashlib.sha256(args.destination.read_bytes()).hexdigest(),
        'raw_console_sha256': source['raw_console_sha256'],
        'capture_tool_sha256': source.get('capture_tool_sha256'),
        'evidence': source['evidence'],
        'internal_callback_order': 'unavailable',
        'cases': len(actual),
        'samples': sum(len(case['samples']) for case in source['cases']),
    }
    meta.write_text(json.dumps(metadata, indent=2) + '\n')
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()
