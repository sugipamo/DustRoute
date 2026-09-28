#!/usr/bin/env python3
"""Declare support-loss trials without deriving expected results from a model."""
import argparse
import json
from pathlib import Path

from make_device_circuit_fixtures import position, rotate


def circuit(kind, body_top=False):
    blocks = []

    def add(x, y, z, name, **properties):
        blocks.append(dict(pos=position(x, y, z), name='minecraft:' + name,
                           properties={k: str(v).lower() for k, v in properties.items()}))

    add(0, 0, 0, 'piston', facing='east', extended=False)
    add(-2, 0, 0, 'stone')
    add(-1, 0, 0, 'lever', face='wall', facing='east', powered=False)
    x = 0 if body_top else 3
    if not body_top:
        for support_x in range(1, 4):
            add(support_x, 0, 0, 'stone')
    wall = kind in ('wall-torch', 'lever', 'button')
    y, z = (0, 1) if wall else (1, 0)
    if kind in ('wire', 'powered-wire'):
        powered = kind == 'powered-wire'
        add(x, y, z, 'redstone_wire', power=15 if powered else 0, north='none',
            east='side' if powered else 'none', south='none', west='side' if powered else 'none')
        if powered:
            add(x + 1, y, z, 'redstone_block')
    elif kind in ('torch', 'wall-torch'):
        add(x, y, z, 'redstone_wall_torch' if wall else 'redstone_torch',
            lit=True, **({'facing': 'south'} if wall else {}))
    elif kind in ('lever', 'button'):
        add(x, y, z, 'lever' if kind == 'lever' else 'stone_button', face='wall', facing='south', powered=kind == 'lever')
    elif kind in ('repeater', 'comparator'):
        add(x, y, z, kind, facing='west', powered=False,
            **({'delay': 1, 'locked': False} if kind == 'repeater' else {'mode': 'compare'}))
    else:
        raise ValueError(kind)
    # Observe the attachment's disappearance through an actual observer pulse.
    # Its output and lamp are away from the source piston's QC neighborhood.
    if not body_top:
        add(x, y, z + 1, 'observer', facing='north', powered=False)
        add(x, y, z + 2, 'redstone_lamp', lit=False)
    inputs = [position(-1, 0, 0)]
    kinds = ['lever']
    steps = [dict(input=0, powered=True, wait_ticks=0), dict(input=0, powered=False, wait_ticks=40)]
    if kind == 'button':
        inputs.append(position(x, y, z))
        kinds.append('use_device')
        steps[0]['wait_ticks'] = 4
        steps.insert(0, dict(input=1, powered=True, wait_ticks=0))
    if kind in ('repeater', 'comparator'):
        add(2, 1, 0, 'lever', face='floor', facing='east', powered=False)
        inputs.append(position(2, 1, 0))
        kinds.append('lever')
        steps[0]['wait_ticks'] = 4
        steps.insert(0, dict(input=1, powered=True, wait_ticks=0))
    return dict(schema_version='dustroute.mixed-piston-fixture.v1',
                id='device-support-' + ('body-' if body_top else '') + kind,
                initial=dict(blocks=blocks), inputs=inputs, input_kinds=kinds,
                steps=steps, placement_mode='strict', warmup_ticks=100,
                settling_ticks=80, packet_input=True, align_first_input=True,
                shared_viewpoint=True, sample_full_region=True, require_vanilla_features=True, compare_tick_attempts=True,
                initial_hidden_state='fresh stable placement, zero comparator output, empty queue/history')


def pulling_support():
    fixture = circuit('wire')
    fixture['id'] = 'device-support-pull-wire'
    blocks = []
    for block in fixture['initial']['blocks']:
        if block['name'] == 'minecraft:stone' and block['pos']['x'] in (1, 3):
            continue
        if block['pos']['x'] == 3:
            block['pos']['x'] = 2
        if block['name'] == 'minecraft:piston':
            block['name'] = 'minecraft:sticky_piston'
            block['properties']['extended'] = 'true'
        if block['name'] == 'minecraft:lever':
            block['properties']['powered'] = 'true'
        blocks.append(block)
    blocks.append(dict(pos=position(1, 0, 0), name='minecraft:piston_head',
                       properties=dict(facing='east', type='sticky', short='false')))
    fixture['initial']['blocks'] = blocks
    fixture['steps'] = [dict(input=0, powered=False, wait_ticks=0)]
    return fixture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    bases = [circuit(kind) for kind in ('wire', 'powered-wire', 'torch', 'wall-torch', 'lever', 'button', 'repeater', 'comparator')]
    bases += [circuit(kind, True) for kind in ('wire', 'torch')]
    bases.append(pulling_support())
    for base in bases:
        for turns in (0, 1):
            fixture = rotate(base, turns)
            with (args.output_dir / (fixture['id'] + '.json')).open('x') as out:
                json.dump(fixture, out, indent=2)
                out.write('\n')


if __name__ == '__main__':
    main()
