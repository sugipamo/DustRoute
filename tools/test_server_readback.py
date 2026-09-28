"""Server-confirmed stair readback and refusal regressions from native captures."""
import copy
import gzip
import json
from pathlib import Path
import unittest

from compare_device_circuits import observe
from server_readback import confirmed_snapshot
from make_stair_fixtures import cases
from make_device_circuit_fixtures import rotate

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return json.loads(gzip.decompress(path.read_bytes()))


class ServerReadbackEvidence(unittest.TestCase):
    def setUp(self):
        self.capture = load(ROOT / 'crates/dustroute-translate/tests/fixtures/device-circuits/stairs-inner-top-left-r0.json.gz')

    def test_unconfirmed_original_remains_incomplete(self):
        original = load(ROOT / 'docs/evidence/stairs-readback-20260928.diagnostic.json.gz')
        with self.assertRaisesRegex(AssertionError, 'drain readback'):
            observe(original['raw'], original['client'], original['fixture'])

    def test_server_confirms_shape_without_rewriting_original_client_observation(self):
        c = self.capture['client']
        corrections = c['server_readbacks']['final']['readback']['corrections']
        self.assertEqual(len(corrections), 1)
        correction = corrections[0]
        client = next(b for b in c['final'] if b['position'] == correction['pos'])
        actual = next(b for b in confirmed_snapshot(c, 'final')['blocks'] if b['pos'] == correction['pos'])
        self.assertEqual(client['properties']['shape'], 'inner_left')
        self.assertEqual(actual['properties']['shape'], 'straight')
        derived = observe(self.capture['raw_interval'], c, self.capture['fixture'])
        self.assertEqual(json.loads(json.dumps(derived)), self.capture['observed'])

    def test_no_client_fallback_on_missing_or_invalid_server_evidence(self):
        for change in ['missing', 'dimension', 'tick', 'predicate_ticks', 'coverage', 'snapshot', 'cleanup']:
            capture = copy.deepcopy(self.capture)
            c = capture['client']
            receipt = c['server_readbacks']['final']['readback']
            if change == 'missing':
                del c['server_readbacks']
            elif change == 'dimension':
                receipt['dimension'] = 'minecraft:the_nether'
            elif change == 'tick':
                receipt['end_game_tick'] += 1
            elif change == 'coverage':
                receipt['checked_cells'] -= 1
            elif change == 'predicate_ticks':
                receipt['predicate_ticks'] = [receipt['start_game_tick'] + 1]
            elif change == 'snapshot':
                c['server_readbacks']['final']['blocks'].pop()
            else:
                del c['server_readbacks']['empty_after_cleanup']
            with self.subTest(change=change), self.assertRaises((AssertionError, KeyError)):
                observe(capture['raw_interval'], c, capture['fixture'])

    def test_native_predicate_clocks_and_stair_correction_are_retained(self):
        capture = load(ROOT / 'crates/dustroute-translate/tests/fixtures/device-circuits/stairs-predicate-inner-top-left-r0.json.gz')
        for label in ['initial', 'final', 'empty_before_setup', 'empty_after_cleanup']:
            confirmed_snapshot(capture['client'], label)
            receipt = capture['client']['server_readbacks'][label]['readback']
            self.assertTrue(receipt['predicate_ticks'])
            self.assertEqual(set(receipt['predicate_ticks']), {receipt['start_game_tick']})
        self.assertTrue(capture['client']['server_readbacks']['final']['readback']['corrections'])

    def test_public_stair_adoption_placement_restart_and_removal_retain_native_receipts(self):
        evidence = load(ROOT / 'docs/evidence/stairs-public-20260928.json.gz')
        client = evidence['client']
        self.assertEqual(client['status'], 'passed')
        for stage in ['after_adoption', 'after_placement', 'after_removal']:
            self.assertTrue(client['restart'][stage])
        self.assertTrue(all(client['persistence_checks'].values()))
        empty_client = {'known_region': client['known_region'], 'server_readbacks': {
            label: {**client['known_region'], 'blocks': [], 'readback': receipt}
            for label, receipt in client['empty_readbacks'].items()
        }}
        for label in ['before_setup', 'after_public_removal', 'after_cleanup']:
            self.assertFalse(confirmed_snapshot(empty_client, label)['blocks'])
        correction = client['stair_readback']['readbacks'][0]['corrections'][0]
        self.assertEqual(correction['client']['properties']['shape'], 'inner_left')
        self.assertEqual(correction['confirmed_shape'], 'straight')
        self.assertGreater(client['stair_readback']['observed_server_tick_interval'], 0)
        instance, = evidence['instances']
        self.assertEqual(instance['state'], 'removed')
        self.assertEqual(len(instance['attempts']), 2)
        requests = set()
        for attempt in instance['attempts']:
            self.assertEqual(attempt['verified_steps'], attempt['total_steps'])
            self.assertIsNone(attempt['error'])
            self.assertEqual(len(attempt['readbacks']), 1 + 2 * attempt['total_steps'])
            for receipt in attempt['readbacks']:
                self.assertEqual(receipt['checked_cells'], 4785)
                self.assertEqual(receipt['start_game_tick'], receipt['end_game_tick'])
                self.assertEqual(receipt['predicate_ticks'], [receipt['start_game_tick']])
                self.assertNotIn(receipt['request_id'], requests)
                requests.add(receipt['request_id'])

    def test_stair_matrix_exercises_corners_arrival_and_both_wire_step_faces(self):
        for case, turns in cases():
            fixture = rotate(case, turns)
            fixture['require_server_readback'] = True
            name = fixture['id'].removeprefix('device-')
            with self.subTest(capture=name):
                capture = load(ROOT / 'crates/dustroute-translate/tests/fixtures/device-circuits' / (name + '.json.gz'))
                self.assertEqual(capture['fixture'], fixture)
                writes = capture['observed']['writes']
                if '-inner-' in name or '-outer-' in name:
                    self.assertTrue(any(w['before'][0] == w['after'][0] == 'minecraft:quartz_stairs'
                                        and dict(w['before'][1])['shape'] != 'straight'
                                        and dict(w['after'][1])['shape'] == 'straight' for w in writes))
                    if '-inner-' in name:
                        self.assertEqual(sum(w['before'][0] == 'minecraft:lever' and w['after'][0] == 'minecraft:air' for w in writes), 1)
                elif '-arrival-' in name:
                    self.assertTrue(any(w['after'][0] == 'minecraft:smooth_quartz_stairs'
                                        and dict(w['after'][1])['shape'].startswith('outer_') for w in writes))
                else:
                    self.assertTrue(any(w['after'][0] == 'minecraft:redstone_wire'
                                        and dict(w['after'][1])['power'] == '14' for w in writes))
                for label in ['initial', 'final', 'empty_before_setup', 'empty_after_cleanup']:
                    confirmed_snapshot(capture['client'], label)


if __name__ == '__main__':
    unittest.main()
