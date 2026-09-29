#!/usr/bin/env python3
"""Run a bounded mixed-device trial using the existing private server and actor."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

from compare_device_circuits import ReplayOutsideScope, compare_model, observe
from instrumented_server import InstrumentedServer, ensure_private_server
from observe_mixed_pistons import ACTOR, ROOT, digest, save


def compare_capture(paths, fixture):
    """Never classify missing observations or a rejected replay as agreement."""
    observation = None
    try:
        raw = [json.loads(line) for line in paths['.raw.ndjson'].read_text().splitlines()]
        client = json.loads(paths['.client.json'].read_text())
        observation = observe(raw, client, fixture)
    except ReplayOutsideScope as error:
        return dict(classification='unsupported_observation', error=str(error)), observation
    except (AssertionError, ValueError, KeyError, IndexError, TypeError, StopIteration, OSError) as error:
        return dict(classification='observation_incomplete',
                    error=f'{type(error).__name__}: {error}'), observation
    save(paths['.reference.json'], observation)
    trial = dict(initial=observation['initial'], inputs=observation['inputs'], trace=True,
                 verify_restoration=True, device_projection=True,
                 restoration_scope=fixture.get('restoration_scope', 'movement_or_device'))
    save(paths['.input.json'], trial)
    try:
        model = subprocess.run([str(ROOT / 'target/debug/examples/compare_electrical_pistons'), str(paths['.input.json'])],
                               text=True, capture_output=True, timeout=120)
    except subprocess.TimeoutExpired:
        return dict(classification='model_timeout'), observation
    if model.returncode:
        return dict(classification='model_rejected', error=model.stderr), observation
    try:
        predicted = json.loads(model.stdout)
        save(paths['.model.json'], predicted)
        result = compare_model(observation, trial, predicted)
    except (AssertionError, ValueError, KeyError, IndexError, TypeError) as error:
        return dict(classification='model_comparison_incomplete',
                    error=f'{type(error).__name__}: {error}'), observation
    differences = ['tick_mismatches', 'first_write_mismatch', 'first_scheduled_mismatch', 'first_callback_mismatch', 'first_tick_attempt_mismatch']
    result['classification'] = 'mismatch' if any(result[k] for k in differences) else 'matched_declared_projection'
    return result, observation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--x', type=int, required=True)
    args = parser.parse_args()
    assert args.run_id.replace('-', '').replace('_', '').isalnum()
    fixture = json.loads(args.fixture.read_text())
    assert fixture['require_vanilla_features'] and fixture['placement_mode'] == 'strict'
    assert fixture.get('compare_drain_ticks', 40) + 20 <= fixture['settling_ticks'] <= 200
    capture_ticks = sum(s['wait_ticks'] for s in fixture['steps']) + fixture['settling_ticks'] + 40
    assert 1 <= capture_ticks <= 1200
    prefix = ROOT / '.local/e2e-artifacts' / args.run_id
    prefix.parent.mkdir(parents=True, exist_ok=True)
    paths = {s: prefix.with_suffix(s) for s in ('.raw.ndjson', '.server.log', '.client.json', '.reference.json', '.input.json', '.model.json', '.comparison.json', '.summary.json')}
    assert not any(p.exists() for p in paths.values()), 'immutable artifacts already exist'
    instrumentation = Path('/root/DustRoute-minecraft-instrumentation')
    ensure_private_server(instrumentation)
    origin = dict(x=args.x, y=180, z=1000)
    bounds = [fixture['initial'][e][a] + origin[a] for e in ('min', 'max') for a in ('x', 'y', 'z')]
    provenance = dict(fixture_sha256=digest(args.fixture), actor_sha256=digest(ACTOR),
                      readback_sha256=digest(ACTOR.parent.parent / 'readback.js'),
                      readback_validator_sha256=digest(Path(__file__).with_name('server_readback.py')),
                      capture_sha256=digest(Path(__file__)), comparator_sha256=digest(Path(__file__).with_name('compare_device_circuits.py')),
                      projection_sha256=digest(Path(__file__).with_name('compare_piston_transients.py')),
                      model_binary_sha256=digest(ROOT / 'target/debug/examples/compare_electrical_pistons'))
    print('Starting private mixed-device capture', flush=True)
    server = InstrumentedServer(instrumentation, paths['.raw.ndjson'], paths['.server.log'], capture_ticks, bounds)
    capture_error = None
    try:
        env = {**os.environ, 'DUSTROUTE_MIXED_FIXTURE': str(args.fixture.resolve()),
               'DUSTROUTE_MIXED_OUTPUT': str(paths['.client.json']), 'DUSTROUTE_MIXED_X': str(args.x)}
        actor = subprocess.run(['node', str(ACTOR)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=190)
        print(actor.stdout, end='', flush=True)
        if actor.returncode:
            capture_error = actor.stderr or 'actor failed; inspect client artifact'
        else:
            time.sleep(capture_ticks / 20 + 2)
    except subprocess.TimeoutExpired:
        capture_error = 'actor timeout; inspect cleanup evidence'
    finally:
        server.close()
    client = json.loads(paths['.client.json'].read_text()) if paths['.client.json'].exists() else {}
    observation = None
    if capture_error:
        result = dict(classification='observation_incomplete', error=client.get('error', capture_error))
    else:
        result, observation = compare_capture(paths, fixture)
    save(paths['.comparison.json'], result)
    summary = dict(fixture=fixture['id'], **provenance, classification=result['classification'],
                   applied_inputs=observation['applied_inputs'] if observation else None, requested_steps=fixture['steps'],
                   cleanup=client.get('cleanup'), server_stopped_normally=True,
                   enabled_features=client.get('enabled_features'),
                   artifacts={s: dict(path=str(p), sha256=digest(p)) for s, p in paths.items() if p.exists()})
    save(paths['.summary.json'], summary)
    print(json.dumps(dict(classification=result['classification'], summary=str(paths['.summary.json']),
                          first_write_mismatch=result.get('first_write_mismatch'), first_scheduled_mismatch=result.get('first_scheduled_mismatch'),
                          first_tick_attempt_mismatch=result.get('first_tick_attempt_mismatch'),
                          first_callback_mismatch=result.get('first_callback_mismatch'), error=result.get('error'))), flush=True)
    if result['classification'] != 'matched_declared_projection':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
