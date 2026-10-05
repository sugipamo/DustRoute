import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from fixture_test_executable import build_fixture_test, fixture_command


class FixtureExecutableTests(unittest.TestCase):
    def test_selects_the_exact_reported_test_and_does_not_run_it(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'fixture-test'
            binary.write_text('not executed')
            record = {'reason': 'compiler-artifact', 'target': {'name': 'probe'},
                      'profile': {'test': True}, 'executable': str(binary)}
            result = subprocess.CompletedProcess([], 0, json.dumps(record)+'\n')
            with patch('fixture_test_executable.subprocess.run', return_value=result) as run:
                actual = build_fixture_test('dustroute-mcp', 'probe')
            self.assertEqual(actual, binary)
            command = run.call_args.args[0]
            self.assertIn('--no-run', command)
            self.assertIn('--offline', command)
            self.assertIn('--locked', command)
            self.assertIn('-j1', command)
            self.assertEqual(fixture_command(actual)[1:], ['--ignored', '--exact',
                             'retain_fixture', '--nocapture', '--test-threads=1', '--format=terse'])

    def test_missing_or_ambiguous_artifacts_are_not_replaced_by_a_stale_binary(self):
        record = {'reason': 'compiler-artifact', 'target': {'name': 'probe'},
                  'profile': {'test': True}, 'executable': '/unavailable/probe'}
        for output in ['', '\n'.join([json.dumps(record)] * 2)]:
            with patch('fixture_test_executable.subprocess.run',
                       return_value=subprocess.CompletedProcess([], 0, output)):
                with self.assertRaises(RuntimeError):
                    build_fixture_test('dustroute-mcp', 'probe')


if __name__ == '__main__':
    unittest.main()
