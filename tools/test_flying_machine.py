"""Finite-flight requirements checked against independent server evidence."""
import copy
import gzip
import json
from pathlib import Path
import unittest

from compare_device_circuits import observe
from flying_machine_trial import fixture, flight_progress


class FlyingMachineEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(__file__).resolve().parents[1] / 'crates/dustroute-translate/tests/fixtures/device-circuits/flying-short-course.json.gz'
        cls.capture = json.loads(gzip.decompress(path.read_bytes()))

    def test_live_journey_is_intact_at_each_displacement_and_stops(self):
        capture = self.capture
        self.assertEqual(fixture(), capture['fixture'])
        observed = observe(capture['raw_interval'], capture['client'], capture['fixture'])
        self.assertEqual(json.loads(json.dumps(observed)), capture['observed'])
        progress = flight_progress(observed, capture['client']['origin'])
        self.assertEqual(progress['engine_blocks'], 6)
        self.assertEqual(progress['distance'], 10)
        self.assertEqual([f['displacement'] for f in progress['intact_frames']], list(range(11)))
        self.assertGreaterEqual(progress['quiet_ticks'], 40)

    def test_arrival_with_a_missing_part_or_leftover_debris_is_not_a_pass(self):
        for damage in ['missing_part', 'leftover_debris']:
            with self.subTest(damage=damage):
                observed = copy.deepcopy(self.capture['observed'])
                final = observed['tick_end_worlds'][-1]['blocks']
                if damage == 'missing_part':
                    final.remove(next(b for b in final if b[1][0] == 'minecraft:observer'))
                else:
                    origin = self.capture['client']['origin']
                    final.append([[origin['x'], origin['y'], origin['z']], ['minecraft:slime_block', []]])
                    final.sort()
                with self.assertRaisesRegex(AssertionError, 'wrong final region'):
                    flight_progress(observed, self.capture['client']['origin'])

    def test_arrival_alone_without_a_quiet_interval_is_not_a_stop(self):
        observed = copy.deepcopy(self.capture['observed'])
        last_write = max(w['tick'] for w in observed['writes'])
        observed['tick_end_worlds'] = [s for s in observed['tick_end_worlds'] if s['tick'] <= last_write + 1]
        with self.assertRaisesRegex(AssertionError, 'quiet interval'):
            flight_progress(observed, self.capture['client']['origin'])


if __name__ == '__main__':
    unittest.main()
