#!/usr/bin/env python3
"""Replay retained independent device observations and reject weakened evidence."""
import copy
import gzip
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from compare_device_circuits import compare_model, observe
from make_device_circuit_fixtures import locking_circuit, rotate, torch_feedback
from make_passive_shape_fixtures import cases as shape_cases
from observe_device_circuit import compare_capture

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'crates/dustroute-translate/tests/fixtures/device-circuits'
MODEL = ROOT / 'target/debug/examples/compare_electrical_pistons'


def load(path):
    return json.loads(gzip.decompress(path.read_bytes()))


def trial_for(observed, projected=True, restoration_scope='movement_or_device'):
    return dict(initial=observed['initial'], inputs=observed['inputs'], trace=True,
                verify_restoration=True, device_projection=projected,
                restoration_scope=restoration_scope)


def replay(trial):
    with tempfile.TemporaryDirectory(prefix='dustroute-device-replay-') as directory:
        path = Path(directory) / 'input.json'
        path.write_text(json.dumps(trial))
        result = subprocess.run([str(MODEL), str(path)], capture_output=True, text=True, timeout=120)
    return result


class DeviceCircuitEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = load(FIXTURES / 'bulb-compare-r0.json.gz')

    def test_all_retained_observations_rederive_and_replay_with_restoration(self):
        paths = sorted(FIXTURES.glob('*.json.gz'))
        self.assertTrue(paths)
        for path in paths:
            with self.subTest(capture=path.name):
                captured = load(path)
                actual = observe(captured['raw_interval'], captured['client'], captured['fixture'])
                self.assertEqual(json.loads(json.dumps(actual)), captured['observed'])
                trial = trial_for(actual, restoration_scope=captured['fixture'].get('restoration_scope', 'movement_or_device'))
                run = replay(trial)
                self.assertEqual(run.returncode, 0, run.stderr)
                result = compare_model(actual, trial, json.loads(run.stdout))
                self.assertEqual(result['tick_mismatches'], [])
                for field in ['first_write_mismatch', 'first_scheduled_mismatch', 'first_callback_mismatch', 'first_tick_attempt_mismatch']:
                    self.assertIsNone(result[field], (path.name, field, result[field]))

    def test_input_tick_is_not_inferred_from_the_requested_client_delay(self):
        capture = copy.deepcopy(self.base)
        first = next(r for r in capture['raw_interval'] if r['kind'] == 'input_packet')
        first['game_tick'] += 1
        with self.assertRaises(AssertionError):
            observe(capture['raw_interval'], capture['client'], capture['fixture'])

    def test_nonmoving_trial_requires_explicit_input_notification_restoration(self):
        capture = load(FIXTURES / 'adhesion-blocked.json.gz')
        strict = replay(trial_for(capture['observed']))
        self.assertNotEqual(strict.returncode, 0)
        self.assertIn('suspended movement/device checkpoint', strict.stderr)
        explicit = replay(trial_for(capture['observed'], restoration_scope='input_notifications'))
        self.assertEqual(explicit.returncode, 0, explicit.stderr)
        model = json.loads(explicit.stdout)
        self.assertTrue(model['restoration_verified'])
        self.assertEqual(model['restoration_scope'], 'input_notifications')
        moving = load(FIXTURES / 'adhesion-slime.json.gz')
        rejected = replay(trial_for(moving['observed'], restoration_scope='input_notifications'))
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn('unexpectedly moved', rejected.stderr)

    def test_missing_commit_or_heartbeat_never_counts_as_agreement(self):
        for kind in ['state_commit', 'server_world_tick']:
            with self.subTest(kind=kind):
                capture = copy.deepcopy(self.base)
                index = next(i for i, r in enumerate(capture['raw_interval']) if i > 1 and r['kind'] == kind)
                del capture['raw_interval'][index]
                with self.assertRaises(AssertionError):
                    observe(capture['raw_interval'], capture['client'], capture['fixture'])

    def test_wrong_features_or_unclean_capture_are_rejected(self):
        for field in ['features', 'cleanup', 'write_errors']:
            capture = copy.deepcopy(self.base)
            if field == 'features':
                capture['client']['enabled_features'].append('minecraft:redstone_experiments')
            elif field == 'cleanup':
                capture['client']['cleanup']['region_empty'] = False
            else:
                capture['raw_interval'][-1]['write_errors'] = 1
            with self.subTest(field=field), self.assertRaises(AssertionError):
                observe(capture['raw_interval'], capture['client'], capture['fixture'])

    def test_full_and_reduced_trace_compare_the_same_callbacks(self):
        observation = self.base['observed']
        outcomes = []
        for projected in [False, True]:
            trial = trial_for(observation, projected)
            result = replay(trial)
            self.assertEqual(result.returncode, 0, result.stderr)
            outcomes.append(compare_model(observation, trial, json.loads(result.stdout)))
        self.assertEqual(outcomes[0], outcomes[1])
        self.assertGreater(outcomes[0]['live_callback_count'], 0)

    def test_visible_final_agreement_does_not_hide_callback_or_write_differences(self):
        observation = copy.deepcopy(self.base['observed'])
        trial = trial_for(observation)
        run = replay(trial)
        self.assertEqual(run.returncode, 0, run.stderr)
        model = json.loads(run.stdout)
        observation['callbacks'][0]['position'][0] += 1
        observation['writes'][0]['tick'] += 1
        result = compare_model(observation, trial, model)
        self.assertEqual(result['tick_mismatches'], [])
        self.assertIsNotNone(result['first_callback_mismatch'])
        self.assertIsNotNone(result['first_write_mismatch'])

    def test_ambiguous_device_use_or_missing_lever_level_is_rejected(self):
        for change in [dict(powered=True), dict(after_world_tick=False), dict(use_device=False)]:
            trial = trial_for(copy.deepcopy(self.base['observed']))
            trial['inputs'][0].update(change)
            run = replay(trial)
            self.assertNotEqual(run.returncode, 0)

    def test_discarded_button_tick_is_compared_even_without_a_visible_write(self):
        observation = load(FIXTURES / 'support-button-r0.json.gz')['observed']
        discarded = [a for a in observation['tick_attempts'] if not a['delivered']]
        self.assertEqual(len(discarded), 1)
        trial = trial_for(observation)
        run = replay(trial)
        self.assertEqual(run.returncode, 0, run.stderr)
        model = json.loads(run.stdout)
        self.assertIsNone(compare_model(observation, trial, model)['first_tick_attempt_mismatch'])
        weakened = copy.deepcopy(observation)
        weakened['tick_attempts'].remove(discarded[0])
        result = compare_model(weakened, trial, model)
        self.assertEqual(result['tick_mismatches'], [])
        self.assertIsNone(result['first_write_mismatch'])
        self.assertIsNone(result['first_scheduled_mismatch'])
        self.assertIsNotNone(result['first_tick_attempt_mismatch'])

    def test_declared_case_coverage_is_present_in_server_writes(self):
        required = {'bulb-compare-r0', 'bulb-compare-r180', 'bulb-subtract-r90',
                    'bulb-subtract-r270', 'torch-feedback-r0', 'torch-feedback-r90',
                    'torch-separated-r0', 'locking-data-first-r0',
                    'locking-data-first-r90', 'locking-lock-first-r0'}
        paths = {p.name.removesuffix('.json.gz'): p for p in FIXTURES.glob('*.json.gz')}
        self.assertTrue(required <= paths.keys(), required - paths.keys())
        for name in sorted(required):
            with self.subTest(capture=name):
                record = load(paths[name])
                observed = record['observed']
                writes = observed['writes']
                if name.startswith('bulb-'):
                    desired = '2' if 'subtract' in name else '15'
                    self.assertTrue(any(w['after'][0] == 'minecraft:redstone_wire'
                                        and dict(w['after'][1])['power'] == desired for w in writes))
                    button = [w for w in writes if w['after'][0] == 'minecraft:stone_button']
                    self.assertEqual(len(button), 8)
                    for on, off in zip(button[::2], button[1::2]):
                        self.assertEqual(off['tick'] - on['tick'], 20)
                        self.assertEqual(dict(off['after'][1])['powered'], 'false')
                if name.startswith('locking-'):
                    applied = observed['applied_inputs']
                    if name.endswith('r0'):
                        self.assertEqual(applied[0]['game_tick'], applied[1]['game_tick'])
                    self.assertTrue(any(b['game_tick'] - a['game_tick'] == 1
                                        for a, b in zip(applied, applied[1:])))
                    self.assertTrue(any(w['after'][0] == 'minecraft:repeater'
                                        and dict(w['after'][1])['locked'] == 'true' for w in writes))
                    # A force-finish before the carrier's final tick is observed,
                    # rather than inferred from a stationary final piston.
                    self.assertTrue(any(e['kind'] == 'finish_begin' and e['carrier']
                                        and e['carrier']['last_progress'] < 1 for e in observed['callbacks']))
                if name.startswith('torch-'):
                    positions = {tuple(w['position']) for w in writes if w['after'][0] == 'minecraft:redstone_wall_torch'}
                    self.assertEqual(len(positions), 2)
                    if name == 'torch-separated-r0':
                        for position in positions:
                            transitions = [w for w in writes if tuple(w['position']) == position]
                            self.assertEqual(len(transitions), 32)
                            self.assertEqual(dict(transitions[15]['after'][1])['lit'], 'false')
                            self.assertGreater(transitions[16]['tick'] - transitions[15]['tick'], 160)
                            self.assertEqual(dict(transitions[16]['after'][1])['lit'], 'true')

    def test_shared_viewpoint_rotates_with_all_inputs(self):
        for base in [torch_feedback(), locking_circuit(), *[f for f, _ in shape_cases()]]:
            for turns in range(4):
                fixture = rotate(base, turns)
                point = fixture['viewpoint']
                for control in fixture['inputs']:
                    distance_squared = sum((control[a] - point[a]) ** 2 for a in ('x', 'y', 'z'))
                    self.assertLessEqual(distance_squared, 16)

    def test_passive_shape_captures_exercise_conduction_steps_and_detachment(self):
        for fixture, turns in shape_cases():
            fixture = rotate(fixture, turns)
            name = fixture['id'].removeprefix('device-')
            with self.subTest(case=name):
                capture = load(FIXTURES / (name + '.json.gz'))
                self.assertEqual(capture['fixture'], fixture)
                observed = capture['observed']
                writes = observed['writes']
                if 'conduction-' in name:
                    conducting = 'double' in name
                    self.assertEqual(any(w['after'][0] == 'minecraft:redstone_wire'
                                         and dict(w['after'][1])['power'] == '15' for w in writes), conducting)
                    self.assertEqual(any(w['after'][0] == 'minecraft:redstone_lamp'
                                         and dict(w['after'][1])['lit'] == 'true' for w in writes), conducting)
                elif 'step-' in name:
                    origin = capture['client']['origin']
                    powers = {1: [], 2: []}
                    for tick in observed['tick_end_worlds']:
                        # Inspect the second input alone, after lower input OFF.
                        if observed['inputs'][2]['tick'] < tick['tick'] < observed['inputs'][3]['tick']:
                            for pos, state in tick['blocks']:
                                if state[0] == 'minecraft:redstone_wire':
                                    powers[pos[1] - origin['y']].append(int(dict(state[1])['power']))
                    self.assertEqual(max(powers[2]), 15)
                    self.assertEqual(max(powers[1]), 14 if 'double' in name else 0)
                    # First input alone must reach both levels for every support.
                    self.assertTrue(any(w['after'][0] == 'minecraft:redstone_wire'
                                        and w['position'][1] == origin['y'] + 2
                                        and dict(w['after'][1])['power'] == '14'
                                        and w['tick'] < observed['inputs'][1]['tick'] for w in writes))
                else:
                    kind = 'stone_button' if 'bottom' in name else 'redstone_wire'
                    removals = [w for w in writes if w['before'][0] == 'minecraft:' + kind
                                and w['after'][0] == 'minecraft:air']
                    self.assertEqual(len(removals), 1)
                    slab = next(b for b in fixture['initial']['blocks'] if b['name'].endswith('_slab'))
                    final = observed['tick_end_worlds'][-1]['blocks']
                    self.assertTrue(any(s[0] == slab['name'] and dict(s[1]) == slab['properties'] for _, s in final))
                    self.assertTrue(any(w['after'][0] == 'minecraft:redstone_lamp'
                                        and dict(w['after'][1])['lit'] == 'true' for w in writes))
                    if kind == 'stone_button':
                        self.assertEqual(dict(removals[0]['before'][1])['powered'], 'true')
                        self.assertEqual(sum(not a['delivered'] for a in observed['tick_attempts']), 1)

    def test_support_captures_exercise_removal_and_centered_survival(self):
        cases = {'wire-r0': 'redstone_wire', 'powered-wire-r0': 'redstone_wire',
                 'torch-r0': 'redstone_torch', 'wall-torch-r90': 'redstone_wall_torch',
                 'lever-r0': 'lever', 'button-r0': 'stone_button',
                 'repeater-r0': 'repeater', 'comparator-r0': 'comparator',
                 'pull-wire-r90': 'redstone_wire',
                 'body-wire-r0': 'redstone_wire', 'body-torch-r0': 'redstone_torch'}
        for case, kind in cases.items():
            with self.subTest(case=case):
                observed = load(FIXTURES / ('support-' + case + '.json.gz'))['observed']
                removals = [w for w in observed['writes'] if w['before'][0] == 'minecraft:' + kind
                            and w['after'][0] == 'minecraft:air']
                self.assertEqual(len(removals), 0 if case == 'body-torch-r0' else 1)
                final = {tuple(p): s for p, s in observed['tick_end_worlds'][-1]['blocks']}
                if case == 'body-torch-r0':
                    self.assertTrue(any(s[0] == 'minecraft:redstone_torch' for s in final.values()))
                    self.assertTrue(any(w['after'][0] == 'minecraft:piston'
                                        and dict(w['after'][1])['extended'] == 'true' for w in observed['writes']))
                    continue
                removed = removals[0]
                properties = dict(removed['before'][1])
                if case in ['button-r0', 'lever-r0', 'repeater-r0', 'comparator-r0']:
                    self.assertEqual(properties['powered'], 'true')
                if case == 'powered-wire-r0':
                    self.assertEqual(properties['power'], '15')
                self.assertNotIn(tuple(removed['position']), final)
                if case == 'pull-wire-r90':
                    x, y, z = removed['position']
                    self.assertNotIn((x, y - 1, z), final)
                    self.assertEqual(final[(x, y - 1, z - 1)][0], 'minecraft:stone')
                    continue
                if not case.startswith('body-'):
                    self.assertTrue(any(w['after'][0] == 'minecraft:redstone_lamp'
                                        and dict(w['after'][1])['lit'] == 'true' for w in observed['writes']))
                    # Later solid support at the same location does not recreate
                    # a component that already lost support during movement.
                    x, y, z = removed['position']
                    dx, dy, dz = (0, -1, 0)
                    if kind in ['redstone_wall_torch', 'lever', 'stone_button']:
                        dx, dy, dz = {'north': (0, 0, 1), 'south': (0, 0, -1),
                                      'west': (1, 0, 0), 'east': (-1, 0, 0)}[properties['facing']]
                    self.assertEqual(final[(x + dx, y + dy, z + dz)][0], 'minecraft:stone')

    def test_missing_unsupported_and_rejected_runs_have_distinct_outcomes(self):
        for kind, expected in [('missing', 'observation_incomplete'),
                               ('features', 'unsupported_observation'),
                               ('model', 'model_rejected')]:
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                paths = {s: Path(directory) / ('capture' + s) for s in
                         ['.raw.ndjson', '.client.json', '.reference.json', '.input.json', '.model.json']}
                record = copy.deepcopy(self.base)
                if kind == 'features':
                    record['client']['enabled_features'].append('minecraft:redstone_experiments')
                if kind != 'missing':
                    paths['.raw.ndjson'].write_text(''.join(json.dumps(r) + '\n' for r in record['raw_interval']))
                    paths['.client.json'].write_text(json.dumps(record['client']))
                with patch('observe_device_circuit.subprocess.run', return_value=
                           subprocess.CompletedProcess([], 1, stdout='', stderr='explicit rejection')) as run:
                    outcome, _ = compare_capture(paths, record['fixture'])
                self.assertEqual(outcome['classification'], expected)
                self.assertEqual(run.call_count, int(kind == 'model'))


if __name__ == '__main__':
    unittest.main()
