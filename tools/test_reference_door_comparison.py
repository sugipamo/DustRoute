"""Offline evidence checks using the native Rust fixture adapter; no server."""
from electrical_fixture_adapter import replay_fixture

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from compare_reference_door import compare
from compare_piston_transients import model_trace, model_writes, compare as compare_events


class ReferenceComparisonTests(unittest.TestCase):
    def setUp(self):
        p = dict(x=0, y=0, z=0)
        snapshot = dict(min=p, max=p, blocks=[])
        self.reference = dict(schema_version='dustroute.reference-door-observation.v1',
                              initial=snapshot, inputs=[dict(position=p, tick=0, powered=True)],
                              world_states={'empty': [], 'solid': [[[0, 0, 0], ['minecraft:stone', []]]]},
                              tick_end_worlds=[dict(tick=0, world='empty'), dict(tick=1, world='solid')])
        self.trial = dict(initial=snapshot, inputs=[dict(position=p, tick=0, powered=True, after_world_tick=True)])
        change = dict(position=p, before=dict(kind='Air'),
                      after=dict(kind='Solid', observed_name='minecraft:stone', observed_properties={}))
        self.model = dict(status=dict(kind='complete'), pending=0, profile='test', restoration_verified=False,
                          trace=[dict(invocation=dict(time=dict(game_tick=0, section='after_world_tick')),
                                      delta=dict(changes=[change]))])

    def test_post_world_input_is_visible_at_the_next_tick_end(self):
        result = compare(self.reference, self.trial, self.model)
        self.assertEqual(result['classification'], 'matched_tick_end_states')
        self.model['trace'][0]['invocation']['time']['section'] = 'external'
        result = compare(self.reference, self.trial, self.model)
        self.assertEqual(result['first_mismatch']['tick'], 0)

    def test_different_input_missing_tick_and_truncated_trace_cannot_pass(self):
        trial = copy.deepcopy(self.trial)
        trial['inputs'][0]['after_world_tick'] = False
        with self.assertRaisesRegex(AssertionError, 'input boundaries'):
            compare(self.reference, trial, self.model)
        reference = copy.deepcopy(self.reference)
        reference['tick_end_worlds'][1]['tick'] = 2
        with self.assertRaises(AssertionError):
            compare(reference, self.trial, self.model)
        model = copy.deepcopy(self.model)
        model['trace'][0]['invocation']['time']['game_tick'] = 3
        with self.assertRaisesRegex(AssertionError, 'outside retained'):
            compare(self.reference, self.trial, model)


class RetainedDoorTests(unittest.TestCase):
    def test_recorded_door_tick_ends_callback_worlds_and_saved_continuations(self):
        root = Path(__file__).resolve().parents[1]
        fixtures = root / 'crates/dustroute-translate/tests/fixtures'
        reference_path = fixtures / 'reference-3x3-observed-a-v1.json'
        reference = json.loads(reference_path.read_text())
        callbacks = json.loads((fixtures / 'reference-3x3-callbacks-a-v1.json').read_text())
        self.assertEqual(callbacks['reference_sha256'], hashlib.sha256(reference_path.read_bytes()).hexdigest())
        observed = []
        for digest, world in callbacks['world_states'].items():
            self.assertEqual(digest, hashlib.sha256(json.dumps(world, separators=(',', ':')).encode()).hexdigest())
        for event in callbacks['events']:
            observed.append(dict(event, world=callbacks['world_states'][event['world']]))
        trial = dict(initial=reference['initial'], inputs=[dict(i, after_world_tick=True) for i in reference['inputs']],
                     trace=True, verify_restoration=True)
        with tempfile.TemporaryDirectory() as directory:
            request = Path(directory) / 'input.json'
            request.write_text(json.dumps(trial))
            run = replay_fixture(request, timeout=180)
        self.assertEqual(run.returncode, 0, run.stderr)
        model = json.loads(run.stdout)
        self.assertTrue(model['restoration_verified'])
        self.assertEqual(compare(reference, trial, model)['mismatched_ticks'], 0)
        writes = [{k: c[k] for k in ('tick', 'position', 'before', 'after')} for c in reference['state_commits']]
        self.assertIsNone(compare_events(writes, model_writes(model['trace'])))
        initialization = {r['invocation']['root'] for r in model['trace'] if r['invocation']['call']['payload'] == 'Initialize'}
        predicted = model_trace(reference['initial'], [r for r in model['trace'] if r['invocation']['root'] not in initialization], first_tick=0)
        self.assertIsNone(compare_events(observed, predicted))


if __name__ == '__main__':
    unittest.main()
