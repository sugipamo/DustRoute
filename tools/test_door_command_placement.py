import copy
import json
from pathlib import Path
import unittest

from analyze_door_command_placement import analyze


class CommandPlacementEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = json.loads((Path(__file__).resolve().parents[1] /
            'crates/dustroute-translate/tests/fixtures/reference-door-command-placement-v1.json').read_text())

    def test_prewrite_observer_schedule_and_displacement_are_retained(self):
        result = analyze(self.data)
        self.assertEqual(result['attempted_stage'], 38)
        self.assertEqual(len(result['differences']), 2)
        self.assertFalse(result['live_construction_passed'])
        self.assertFalse(result['completion_certified'])

    def test_missing_or_reordered_capture_and_changed_delayed_readback_are_rejected(self):
        bad = copy.deepcopy(self.data)
        del bad['raw_window'][5]
        with self.assertRaises(AssertionError): analyze(bad)
        bad = copy.deepcopy(self.data)
        pending = next(r for r in bad['raw_window'] if r['kind'] == 'observer_callback' and r.get('event') == 'schedule_after')
        pending['sequence'] += 1000
        with self.assertRaises(AssertionError): analyze(bad)
        bad = copy.deepcopy(self.data)
        bad['failure_observations'][1]['snapshot']['blocks'] = bad['attempted_step']['expected']['blocks']
        with self.assertRaises(AssertionError): analyze(bad)
        bad = copy.deepcopy(self.data)
        bad['capture_header']['capture_mode'] = 'bounded'
        with self.assertRaises(AssertionError): analyze(bad)


if __name__ == '__main__':
    unittest.main()
