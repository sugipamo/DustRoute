"""Offline replay of retained short-input evidence; never starts Minecraft."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from compare_piston_transients import compare as compare_events, model_trace, model_writes
from compare_reference_door import compare
from reference_door_analysis import analyze

ROOT = Path(__file__).resolve().parents[1]


class InterruptedDoorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = ROOT / 'crates/dustroute-translate/tests/fixtures/reference-door-short-live-v1.json'
        cls.retained = json.loads(path.read_text())
        assert cls.retained['schema_version'] == 'dustroute.reference-door-short-live.v1'

    def test_measured_pulses_match_tick_ends_writes_callbacks_and_restored_execution(self):
        binary = Path(os.environ.get('DUSTROUTE_TRANSIENT_MODEL', ROOT / 'target/debug/examples/compare_electrical_pistons'))
        self.assertTrue(binary.is_file(), 'Build compare_electrical_pistons first')
        for case in self.retained['cases']:
            with self.subTest(requested_ticks=case['requested_ticks']):
                reference, callbacks = case['reference'], case['callbacks']
                serialized = json.dumps(reference, indent=2) + '\n'
                self.assertEqual(callbacks['reference_sha256'], hashlib.sha256(serialized.encode()).hexdigest())
                observed = []
                for digest, world in callbacks['world_states'].items():
                    self.assertEqual(digest, hashlib.sha256(json.dumps(world, separators=(',', ':')).encode()).hexdigest())
                for event in callbacks['events']:
                    observed.append(dict(event, world=callbacks['world_states'][event['world']]))
                trial = dict(initial=reference['initial'],
                             inputs=[dict(i, after_world_tick=True) for i in reference['inputs']],
                             trace=True, verify_restoration=True)
                with tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / 'input.json'
                    path.write_text(json.dumps(trial))
                    run = subprocess.run([str(binary), str(path)], text=True, capture_output=True, timeout=180)
                self.assertEqual(run.returncode, 0, run.stderr)
                model = json.loads(run.stdout)
                self.assertTrue(model['restoration_verified'])
                self.assertEqual(compare(reference, trial, model)['mismatched_ticks'], 0)
                writes = [{k: c[k] for k in ('tick', 'position', 'before', 'after')} for c in reference['state_commits']]
                self.assertIsNone(compare_events(writes, model_writes(model['trace'])))
                initialization = {r['invocation']['root'] for r in model['trace'] if r['invocation']['call']['payload'] == 'Initialize'}
                predicted = model_trace(reference['initial'],
                                        [r for r in model['trace'] if r['invocation']['root'] not in initialization], first_tick=0)
                self.assertIsNone(compare_events(observed, predicted))

    def test_negative_outcome_is_retained_but_incomplete_evidence_is_rejected(self):
        capture = self.retained['negative_capture']
        raw, client, fixture = capture['raw'], capture['client'], capture['fixture']
        result = analyze(raw, client, fixture, interrupted=True)
        self.assertEqual(result['outcomes'][0]['aperture'], 'closed')
        self.assertEqual(len(result['outcomes'][0]['occupied_aperture']), 9)
        # The two-input probe must not become a normal four-input certificate.
        with self.assertRaises(AssertionError):
            analyze(raw, client, fixture)
        packets = [r['sequence'] for r in raw if r['kind'] == 'input_packet']
        cut = next(i for i, r in enumerate(raw) if r['kind'] == 'server_world_tick'
                   and r.get('dimension') == 'minecraft:overworld' and r['sequence'] > packets[-1])
        with self.assertRaises(AssertionError):
            analyze(raw[:cut] + raw[cut + 1:], client, fixture, interrupted=True)
        bad = copy.deepcopy(raw)
        write = next(r for r in bad if r['kind'] == 'block_state_change'
                     and r['sequence'] > packets[-1] and 'minecraft:lever' in r['after'])
        write['game_tick'] += 1
        with self.assertRaises(AssertionError):
            analyze(bad, client, fixture, interrupted=True)
        bad_client = copy.deepcopy(client)
        cell = next(b for b in bad_client['final'] if b['name'] == 'minecraft:smooth_quartz')
        cell.update(name='minecraft:air', properties={})
        with self.assertRaises(AssertionError):
            analyze(raw, bad_client, fixture, interrupted=True)


if __name__ == '__main__':
    unittest.main()
