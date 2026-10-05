"""Offline fixture IO regressions; the Cargo process is mocked in these tests."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from electrical_fixture_adapter import replay_fixture


class FixtureAdapterTests(unittest.TestCase):
    def test_a_successful_process_without_a_fixture_cannot_pass(self):
        with patch('electrical_fixture_adapter.subprocess.run', return_value=
                   subprocess.CompletedProcess([], 0, 'test harness output', '')):
            result = replay_fixture('/unused/input.json')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('unverified', result.stderr)
            self.assertEqual(result.stdout, '')
            with self.assertRaises(subprocess.CalledProcessError):
                replay_fixture('/unused/input.json', check=True)

    def test_native_rejection_is_retained_instead_of_harness_success(self):
        def reject(command, **kwargs):
            Path(kwargs['env']['DUSTROUTE_REPLAY_ERROR']).write_text('missing applied input')
            return subprocess.CompletedProcess(command, 101, 'test failed', 'cargo output')
        with patch('electrical_fixture_adapter.subprocess.run', side_effect=reject):
            result = replay_fixture('/unused/input.json')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stderr, 'missing applied input')
        self.assertEqual(result.stdout, '')

    def test_export_uses_an_explicit_serial_test_without_rewriting_the_input(self):
        with tempfile.TemporaryDirectory() as directory:
            input_path = Path(directory) / 'input.json'
            input_path.write_text('independent input bytes')
            def export(command, **kwargs):
                self.assertIn('--offline', command)
                self.assertIn('--locked', command)
                self.assertIn('-j1', command)
                self.assertIn('--ignored', command)
                self.assertIn('--test-threads=1', command)
                self.assertEqual(Path(kwargs['env']['DUSTROUTE_REPLAY_INPUT']), input_path)
                Path(kwargs['env']['DUSTROUTE_REPLAY_OUTPUT']).write_text('fixture bytes')
                return subprocess.CompletedProcess(command, 0, 'test harness output', '')
            with patch('electrical_fixture_adapter.subprocess.run', side_effect=export):
                result = replay_fixture(input_path, check=True)
            self.assertEqual(result.stdout, 'fixture bytes')
            self.assertEqual(input_path.read_text(), 'independent input bytes')


if __name__ == '__main__':
    unittest.main()
