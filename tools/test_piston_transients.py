#!/usr/bin/env python3
"""Offline server-trace regressions. Build compare_electrical_pistons first."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from compare_piston_transients import compare, live_trace, model_trace, live_writes, model_writes, state, world_rows

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = sorted(path for pattern in ('piston-transient-observed-*-v2.json', 'piston-payload-observed-*-v2.json', 'observer-movement-observed-*-v1.json')
                  for path in (ROOT / 'crates/dustroute-translate/tests/fixtures').glob(pattern))
MODEL = Path(os.environ.get('DUSTROUTE_TRANSIENT_MODEL', ROOT / 'target/debug/examples/compare_electrical_pistons'))


def readback(snapshot):
    blocks = {tuple(row['pos'][axis] for axis in ('x', 'y', 'z')): row for row in snapshot['blocks']}
    rows = []
    for x in range(snapshot['min']['x'], snapshot['max']['x'] + 1):
        for y in range(snapshot['min']['y'], snapshot['max']['y'] + 1):
            for z in range(snapshot['min']['z'], snapshot['max']['z'] + 1):
                block = blocks.get((x, y, z), dict(name='minecraft:air', properties={}))
                rows.append(dict(position=dict(x=x, y=y, z=z), name=block['name'], properties=block['properties']))
    return rows


def client_record(fixture):
    return dict(complete=True, cleanup=fixture['cleanup'],
                known_region={edge: fixture['initial'][edge] for edge in ('min', 'max')},
                initial=readback(fixture['initial']), final=readback(fixture['observed_final']),
                activations=[dict(requested_level=i['powered'], position=i['position']) for i in fixture['inputs']])


class TransientEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not FIXTURES:
            raise RuntimeError('No retained server fixtures found')
        cls.reference = json.loads(FIXTURES[0].read_text())

    def test_recorded_callback_worlds_and_moving_state_restoration(self):
        self.assertTrue(MODEL.is_file(), 'Build the compare_electrical_pistons example before running this suite')
        for path in FIXTURES:
            with self.subTest(capture=path.name), tempfile.TemporaryDirectory() as directory:
                fixture = json.loads(path.read_text())
                # Retained expectations must remain derivable from server records,
                # independently of both the model output and saved classifications.
                observed, applied = live_trace(fixture['raw_window'], client_record(fixture))
                self.assertIsNone(compare(fixture['observed_events'], observed))
                request = dict(initial=fixture['initial'], inputs=fixture['inputs'], trace=True, verify_restoration=True)
                input_path = Path(directory) / 'input.json'
                input_path.write_text(json.dumps(request))
                run = subprocess.run([str(MODEL), str(input_path)], text=True, capture_output=True, timeout=60)
                self.assertEqual(run.returncode, 0, run.stderr)
                output = json.loads(run.stdout)
                self.assertTrue(output['restoration_verified'])
                predicted = model_trace(fixture['initial'], output['trace'])
                self.assertIsNone(compare(observed, predicted))
                self.assertIsNone(compare(live_writes(fixture['raw_window'], client_record(fixture), applied), model_writes(output['trace'])))
                actual = {tuple(row['position'][a] for a in ('x', 'y', 'z')): state(row['name'], row['properties'])
                          for row in output['blocks']}
                expected = {tuple(row['pos'][a] for a in ('x', 'y', 'z')): state(row['name'], row['properties'])
                            for row in fixture['observed_final']['blocks']}
                self.assertEqual(world_rows(actual), world_rows(expected))

    def test_an_omitted_record_cannot_be_interpreted_as_no_event(self):
        raw = copy.deepcopy(self.reference['raw_window'])
        del raw[len(raw) // 2]
        with self.assertRaisesRegex(AssertionError, 'gaps'):
            live_trace(raw, client_record(self.reference))

    def test_incomplete_or_different_capture_scope_never_passes(self):
        for field, value in [('write_errors', 1), ('capture_state', 'closed_early')]:
            with self.subTest(field=field):
                raw = copy.deepcopy(self.reference['raw_window'])
                raw[-1][field] = value
                with self.assertRaises(AssertionError):
                    live_trace(raw, client_record(self.reference))
        raw = copy.deepcopy(self.reference['raw_window'])
        raw[0]['trace_bounds'] = '0,0,0,1,1,1'
        with self.assertRaisesRegex(AssertionError, 'scope'):
            live_trace(raw, client_record(self.reference))

    def test_requested_but_unapplied_input_is_not_a_simulator_result(self):
        client = client_record(self.reference)
        client['activations'][0]['requested_level'] = not client['activations'][0]['requested_level']
        with self.assertRaisesRegex(AssertionError, 'applied lever write'):
            live_trace(self.reference['raw_window'], client)

    def test_input_timestamp_cannot_substitute_for_its_world_tick_section(self):
        raw = copy.deepcopy(self.reference['raw_window'])
        preceding = next(row for row in raw if row['kind'] == 'server_world_tick')
        preceding['phase'] = 'begin'
        with self.assertRaisesRegex(ValueError, 'post-world-tick boundary'):
            live_trace(raw, client_record(self.reference))

    def test_full_heartbeat_interval_is_required_even_with_contiguous_sequences(self):
        raw = copy.deepcopy(self.reference['raw_window'])
        raw[0]['heartbeat_mode'] = 'omitted'
        with self.assertRaisesRegex(AssertionError, 'heartbeats'):
            live_trace(raw, client_record(self.reference))
        raw = copy.deepcopy(self.reference['raw_window'])
        first_begin = next(row for row in raw if row['kind'] == 'server_world_tick'
                           and row.get('dimension') == 'minecraft:overworld' and row['phase'] == 'begin')
        first_begin['phase'] = 'end'
        with self.assertRaisesRegex(AssertionError, 'heartbeat interval'):
            live_trace(raw, client_record(self.reference))

    def test_wrong_progress_world_and_order_are_reported_even_if_final_state_matches(self):
        events = self.reference['observed_events']
        progress = next(i for i, event in enumerate(events) if event['kind'] == 'tick_end' and event['carrier'])
        wrong = copy.deepcopy(events)
        wrong[progress]['carrier']['last_progress'] = 0.75
        self.assertEqual(compare(events, wrong)['index'], progress)
        wrong = copy.deepcopy(events)
        wrong[progress]['world'] = []
        self.assertEqual(compare(events, wrong)['index'], progress)
        wrong = copy.deepcopy(events)
        wrong[progress - 1], wrong[progress] = wrong[progress], wrong[progress - 1]
        self.assertEqual(compare(events, wrong)['index'], progress - 1)


if __name__ == '__main__':
    unittest.main()
