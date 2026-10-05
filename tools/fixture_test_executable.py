"""Build an explicit test fixture executable, without running the fixture.

Cargo's JSON here is external build-tool metadata, never DustRoute runtime state.
Use its exact compiler artifact, not a stale glob/legacy example binary.
"""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def build_fixture_test(crate, name):
    result = subprocess.run(
        ['cargo', 'test', '--offline', '--locked', '-j1', '-p', crate,
         '--test', name, '--no-run', '--message-format=json'],
        cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE,
    )
    artifacts = [record for line in result.stdout.splitlines()
                 if (record := json.loads(line)).get('reason') == 'compiler-artifact'
                 and record.get('target', {}).get('name') == name
                 and record.get('profile', {}).get('test') is True
                 and record.get('executable')]
    if len(artifacts) != 1:
        raise RuntimeError('fixture test executable was not uniquely reported by Cargo')
    binary = Path(artifacts[0]['executable']).resolve(strict=True)
    if not binary.is_file():
        raise RuntimeError('Cargo fixture executable is not a file')
    return binary


def fixture_command(binary):
    return [str(binary), '--ignored', '--exact', 'retain_fixture',
            '--nocapture', '--test-threads=1']
