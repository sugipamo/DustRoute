"""Test-only fixture IO for native electrical replay; never a live operation.

JSON is confined to retained fixture files and this explicitly ignored Rust
test. The owning Rust API computes and checks restoration using native state.
Cargo and fixtures run serially, offline and locked. No product CLI is invoked.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def replay_fixture(path, *, timeout=120, check=False, stdout=None):
    if os.environ.get('DUSTROUTE_TRANSIENT_MODEL'):
        raise ValueError('DUSTROUTE_TRANSIENT_MODEL is retired; use the native electrical replay API')
    with tempfile.TemporaryDirectory(prefix='dustroute-electrical-fixture-') as directory:
        directory = Path(directory)
        output = directory / 'result.json'
        error = directory / 'error.txt'
        executable = directory / 'test-executable.txt'
        env = dict(os.environ, DUSTROUTE_REPLAY_INPUT=str(Path(path).resolve()),
                   DUSTROUTE_REPLAY_OUTPUT=str(output), DUSTROUTE_REPLAY_ERROR=str(error),
                   DUSTROUTE_REPLAY_EXECUTABLE=str(executable))
        command = ['cargo', 'test', '--offline', '--locked', '-j1', '-p', 'dustroute-translate',
                   '--test', 'electrical_fixture_adapter', '--', '--ignored', '--exact',
                   'export_fixture', '--test-threads=1']
        run = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=timeout)
        if run.returncode == 0 and output.is_file():
            result = subprocess.CompletedProcess(command, 0, output.read_text(), run.stderr)
        else:
            # Missing output is unknown/failure, never an empty successful replay.
            diagnostic = (error.read_text() if error.is_file() else
                          'fixture adapter completed without a result; replay is unverified'
                          if run.returncode == 0 else run.stderr + run.stdout)
            result = subprocess.CompletedProcess(command, run.returncode or 1, '', diagnostic)
        if executable.is_file():
            result.replay_binary_sha256 = hashlib.sha256(Path(executable.read_text()).read_bytes()).hexdigest()
        if check:
            result.check_returncode()
        if stdout is not None:
            stdout.write(result.stdout)
            result.stdout = None
        return result
