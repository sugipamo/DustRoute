#!/usr/bin/env python3
"""Compare retained mixed-device observations with an offline model."""
from electrical_fixture_adapter import replay_fixture

import json
import subprocess

from compare_device_circuits import ReplayOutsideScope, compare_model, observe
from observation_records import save


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
        model = replay_fixture(paths['.input.json'],
                               timeout=120)
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


