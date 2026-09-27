"""Inspect retained failed construction evidence; never authorizes placement."""
import argparse
import json
from pathlib import Path


def analyze(data):
    assert data['schema_version'] == 'dustroute.reference-door-command-placement.v1'
    assert data['capture_header']['capture_mode'] == 'continuous'
    footer = data['capture_footer']
    assert footer['kind'] == 'artifact_end' and footer['capture_mode'] == 'continuous'
    assert footer['write_errors'] == footer['evicted_records'] == footer['suppressed_records'] == 0
    rows = data['raw_window']
    assert rows and all(b['sequence'] == a['sequence'] + 1 for a, b in zip(rows, rows[1:])), 'capture gap'
    failed = data['failure_response']
    assert not failed['ok'] and failed['status'] == 'needs_inspection'
    assert not failed['retry_allowed'] and not failed['automatic_rollback']
    assert failed['verified_steps'] == data['verified_prefix_steps'] == data['attempted_stage'] - 1
    step = data['attempted_step']
    p = step['position']
    position = f"BlockPos{{x={p['x']}, y={p['y']}, z={p['z']}}}"
    commit = next(r for r in rows if r['kind'] == 'state_commit' and r['position'] == position
                  and r['before'] == 'Block{minecraft:air}' and 'observer' in r['after'])
    pending = next(r for r in rows if r['kind'] == 'observer_callback' and r['position'] == position
                   and r['event'] == 'schedule_after' and r['queued'])
    assert pending['state'] == 'Block{minecraft:air}'
    assert pending['sequence'] < commit['sequence'] and pending['game_tick'] == commit['game_tick']
    ticks = [r for r in rows if r['kind'] == 'ordered_tick' and r['position'] == position]
    assert ticks and all(r['type'] == 'minecraft:observer' for r in ticks)
    assert ticks[0]['sequence'] > commit['sequence']
    assert ticks[0]['trigger_game_tick'] == ticks[0]['execution_game_tick'] == commit['game_tick'] + 2
    changes = [r for r in rows if r['kind'] == 'state_commit' and r['position'] == position]
    assert any(r['game_tick'] == commit['game_tick'] + 2 and 'powered=true' in r['after'] for r in changes)
    assert any(r['game_tick'] == commit['game_tick'] + 4 and 'powered=false' in r['after'] for r in changes)

    def indexed(snapshot):
        return {tuple(b['pos'][a] for a in ('x', 'y', 'z')): (b['name'], b['properties']) for b in snapshot['blocks']}

    expected = indexed(step['expected'])
    observations = data['failure_observations']
    assert [o['client_wait_ticks'] for o in observations] == [0, 100]
    differences = []
    for observation in observations:
        snapshot = observation['snapshot']
        assert snapshot['complete'] and not snapshot['unknown']
        assert snapshot['min'] == step['expected']['min'] and snapshot['max'] == step['expected']['max']
        actual = indexed(snapshot)
        differences.append([{'position': pos, 'expected': expected.get(pos), 'actual': actual.get(pos)}
                            for pos in sorted(expected.keys() | actual.keys()) if expected.get(pos) != actual.get(pos)])
    assert differences[0] and differences[0] == differences[1], 'displacement must persist in the delayed readback'
    assert data['cleanup']['region_empty'] and data['cleanup']['force_load_removed']
    return {
        'classification': 'live_command_placement_counterexample',
        'verified_steps': data['verified_prefix_steps'], 'attempted_stage': data['attempted_stage'],
        'prewrite_schedule_sequence': pending['sequence'], 'install_sequence': commit['sequence'],
        'installation_game_tick': commit['game_tick'],
        'observer_on_game_tick': ticks[0]['execution_game_tick'],
        'differences': differences[0], 'unchanged_after_100_client_ticks': True,
        'completion_certified': False, 'live_construction_passed': False,
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture', type=Path)
    args = parser.parse_args()
    print(json.dumps(analyze(json.loads(args.fixture.read_text())), indent=2))
