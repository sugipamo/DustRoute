#!/usr/bin/env python3
"""Declare passive-shape trials; expected results come only from live captures."""
import argparse
import json
from pathlib import Path
from fixture_geometry import add, position, rotate

from make_support_loss_fixtures import circuit


def base(name):
    return dict(schema_version='dustroute.mixed-piston-fixture.v1', id='device-shapes-' + name,
                initial=dict(blocks=[]), inputs=[position(-2, 1, 0)], input_kinds=['lever'],
                steps=[dict(input=0, powered=True, wait_ticks=0), dict(input=0, powered=False, wait_ticks=16)],
                placement_mode='strict', warmup_ticks=100, settling_ticks=80,
                packet_input=True, align_first_input=True, shared_viewpoint=True,
                sample_full_region=True, require_vanilla_features=True, compare_tick_attempts=True,
                initial_hidden_state='fresh stable placement, zero comparator output, empty queue/history')


def passive(fixture, name, slab_type):
    add(fixture, 0, 1, 0, name, **(dict(type=slab_type, waterlogged=False) if slab_type else {}))


def conduction(name, slab_type=None):
    fixture = base('conduction-' + (slab_type or name))
    for x, z in [(-2, 0), (-1, 0), (0, 1)]:
        add(fixture, x, 0, z, 'stone')
    add(fixture, -2, 1, 0, 'lever', face='floor', facing='east', powered=False)
    add(fixture, -1, 1, 0, 'repeater', facing='west', delay=1, locked=False, powered=False)
    passive(fixture, name, slab_type)
    add(fixture, 1, 1, 0, 'redstone_lamp', lit=False)
    add(fixture, 0, 1, 1, 'redstone_wire', power=0, north='none', east='none', south='none', west='none')
    # An independent piston also exercises checkpoint/root restoration when
    # the tested passive block conducts nothing and has no internal history.
    add(fixture, -2, 0, -1, 'piston', facing='down', extended=False)
    return fixture


def step(name, slab_type=None):
    fixture = base('step-' + (slab_type or name))
    for x, y, z in [(-2, 0, 0), (-1, 0, 0), (0, 1, 1)]:
        add(fixture, x, y, z, 'stone')
    add(fixture, -2, 1, 0, 'lever', face='floor', facing='east', powered=False)
    add(fixture, 0, 2, 1, 'lever', face='floor', facing='north', powered=False)
    passive(fixture, name, slab_type)
    add(fixture, -1, 1, 0, 'redstone_wire', power=0, north='none', south='none', west='side',
        east='side' if slab_type == 'top' else 'up')
    add(fixture, 0, 2, 0, 'redstone_wire', power=0, north='none', south='side', east='none', west='side')
    add(fixture, -2, 0, -1, 'piston', facing='down', extended=False)
    fixture['inputs'].append(position(0, 2, 1))
    fixture['input_kinds'].append('lever')
    fixture['steps'] += [dict(input=1, powered=True, wait_ticks=16), dict(input=1, powered=False, wait_ticks=16)]
    return fixture


def moving_support(ceiling=False):
    fixture = circuit('button' if ceiling else 'wire')
    fixture['id'] = 'device-shapes-moving-' + ('bottom-button' if ceiling else 'top-wire')
    for block in fixture['initial']['blocks']:
        if block['pos'] == position(3, 0, 0):
            block['name'] = 'minecraft:smooth_stone_slab' if ceiling else 'minecraft:stone_slab'
            block['properties'] = dict(type='bottom' if ceiling else 'top', waterlogged='false')
    if ceiling:
        blocks = []
        for block in fixture['initial']['blocks']:
            if block['pos'] == position(2, 0, 0):
                continue
            if block['pos']['x'] == 3:
                block['pos']['x'] = 2
                if block['name'] != 'minecraft:smooth_stone_slab':
                    block['pos']['y'] -= 1
                    block['pos']['z'] -= 1
            if block['name'] == 'minecraft:stone_button':
                block['properties'].update(face='ceiling', facing='north')
            blocks.append(block)
        fixture['initial']['blocks'] = blocks
        fixture['inputs'][1] = position(2, -1, 0)
    return fixture


def cases():
    return [(conduction('stone_slab', t), 0) for t in ('bottom', 'top', 'double')] + [
        (conduction('tinted_glass'), 1), (conduction('red_stained_glass'), 0),
        (step('stone_slab', 'top'), 0), (step('smooth_stone_slab', 'double'), 1),
        (step('tinted_glass'), 0), (moving_support(), 0), (moving_support(True), 1)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for fixture, turns in cases():
        fixture = rotate(fixture, turns)
        with (args.output_dir / (fixture['id'] + '.json')).open('x') as out:
            json.dump(fixture, out, indent=2)
            out.write('\n')


if __name__ == '__main__':
    main()
