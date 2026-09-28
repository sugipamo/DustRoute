#!/usr/bin/env python3
"""A bounded six-block flight trial; progress is derived only from live evidence."""
import argparse
import json
from pathlib import Path

from make_device_circuit_fixtures import position, rotate
from make_passive_shape_fixtures import add
from observe_mixed_pistons import key
from compare_piston_transients import state, world_rows

DISTANCE = 10
ENGINE = {(0, 0, 0), (0, 0, 1), (0, 1, 0),
          (1, 0, 0), (1, 0, 1), (1, 1, 1)}


def fixture():
    trial = dict(
        schema_version='dustroute.mixed-piston-fixture.v1',
        id='device-flying-short-course', initial=dict(blocks=[]),
        inputs=[position(0, 2, 0)], input_kinds=['lever'],
        steps=[dict(input=0, powered=True, wait_ticks=0)],
        placement_mode='strict', warmup_ticks=100, settling_ticks=200,
        compare_drain_ticks=170, packet_input=True, align_first_input=True,
        shared_viewpoint=True, sample_full_region=False,
        require_vanilla_features=True, require_server_readback=True,
        compare_tick_attempts=True,
        initial_hidden_state='stationary six-block engine; unpowered observers; fresh stable placement; empty pending queue/history',
    )
    # Build observers before the engine so their insertion pulses drain before
    # they can power it. The live actor checks the complete stable initial world.
    add(trial, -1, 2, 0, 'stone')
    add(trial, 0, 2, 0, 'lever', face='wall', facing='east', powered=False)
    add(trial, 0, 1, 0, 'observer', facing='up', powered=False)
    add(trial, 1, 1, 1, 'observer', facing='up', powered=False)
    add(trial, DISTANCE + 2, 0, 1, 'obsidian')
    add(trial, 0, 0, 1, 'piston', facing='east', extended=False)
    add(trial, 1, 0, 0, 'sticky_piston', facing='west', extended=False)
    add(trial, 0, 0, 0, 'slime_block')
    add(trial, 1, 0, 1, 'slime_block')
    return rotate(trial, 0)


def flight_progress(observed, origin):
    """Check the declared finite journey, not just model/live agreement.

    This is a diagnostic projection after exact absolute-coordinate comparison.
    An intact frame does not prove that the server's hidden queue is empty.
    """
    initial = {key(b['pos']): state(b['name'], b['properties'])
               for b in observed['initial']['blocks']}
    ox, oy, oz = key(origin)
    engine_positions = {(x + ox, y + oy, z + oz) for x, y, z in ENGINE}
    engine = {p: initial[p] for p in engine_positions}
    stationary = {p: b for p, b in initial.items() if p not in engine_positions}
    inputs = observed['applied_inputs']
    assert len(inputs) == 1 and inputs[0]['kind'] == 'lever' and inputs[0]['powered'] is True
    control = key(inputs[0]['position'])
    assert control == (ox, oy + 2, oz)
    before = stationary[control]
    stationary[control] = state(before[0], dict(before[1]) | {'powered': 'true'})

    def translated(distance):
        return stationary | {(x + distance, y, z): b for (x, y, z), b in engine.items()}

    # JSON normalization keeps retained and freshly derived observations equal.
    normalized = json.loads(json.dumps(observed))
    frames = []
    for distance in range(DISTANCE + 1):
        expected = json.loads(json.dumps(world_rows(translated(distance))))
        matches = [s['tick'] for s in normalized['tick_end_worlds'] if s['blocks'] == expected]
        assert matches, f'no intact whole-region frame at displacement {distance}'
        frames.append(dict(displacement=distance, first_tick=matches[0], last_tick=matches[-1]))
    assert [f['first_tick'] for f in frames] == sorted(f['first_tick'] for f in frames)
    expected_final = json.loads(json.dumps(world_rows(translated(DISTANCE))))
    assert normalized['tick_end_worlds'][-1]['blocks'] == expected_final, 'wrong final region'
    last_write = max(w['tick'] for w in observed['writes'])
    final_tick = observed['tick_end_worlds'][-1]['tick']
    assert final_tick - last_write >= 40, 'insufficient observed quiet interval'
    assert not observed['scheduled_ticks'] or max(t['tick'] for t in observed['scheduled_ticks']) <= last_write
    return dict(distance=DISTANCE, engine_blocks=len(engine), intact_frames=frames,
                last_write_tick=last_write, final_tick=final_tick,
                quiet_ticks=final_tick - last_write,
                scope='complete observed region, one applied input, intact translated frames at every integer displacement, no final debris, no observed writes/device deliveries during the final quiet interval; no hidden-queue or indefinite-flight proof')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    with args.output.open('x') as stream:
        json.dump(fixture(), stream, indent=2)
        stream.write('\n')


if __name__ == '__main__':
    main()
