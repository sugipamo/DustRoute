#!/usr/bin/env python3
"""Declare small circuits; these fixtures contain no simulated expectations."""
import argparse
import json
from pathlib import Path
from fixture_geometry import DIRECTIONS, position, rotate


def bulb_circuit(subtract=False):
    blocks = []

    def add(x, y, z, name, **properties):
        blocks.append(dict(pos=position(x, y, z), name='minecraft:' + name,
                           properties={k: str(v).lower() for k, v in properties.items()}))

    for x in (1, 2):
        add(x, 0, 0, 'stone')
    add(3, 2, 0, 'stone')
    add(0, 1, 0, 'waxed_copper_bulb', lit=False, powered=False)
    add(-1, 1, 0, 'stone_button', face='wall', facing='west', powered=False)
    add(1, 1, 0, 'comparator', facing='west', mode='subtract' if subtract else 'compare', powered=False)
    add(2, 1, 0, 'redstone_wire', power=0, north='none', east='side', south='none', west='side')
    add(3, 1, 0, 'sticky_piston', facing='up', extended=False)
    if subtract:
        add(1, 1, 4, 'redstone_block')
        for z in range(1, 4):
            add(1, 0, z, 'stone')
            add(1, 1, z, 'redstone_wire', power=12 + z,
                north='side', east='none', south='side', west='none')
    return dict(
        schema_version='dustroute.mixed-piston-fixture.v1',
        id='device-bulb-' + ('subtract' if subtract else 'compare'),
        initial=dict(blocks=blocks), inputs=[position(-1, 1, 0)],
        steps=[dict(input=0, powered=True, wait_ticks=t) for t in (0, 26, 26, 26)],
        input_kinds=['use_device'], placement_mode='strict', warmup_ticks=100,
        settling_ticks=80, packet_input=True, align_first_input=True,
        sample_full_region=True, require_vanilla_features=True,
        initial_hidden_state='fresh comparator output zero; no queued work; no torches',
    )


def torch_feedback():
    blocks = []

    def add(x, y, z, name, **properties):
        blocks.append(dict(pos=position(x, y, z), name='minecraft:' + name,
                           properties={k: str(v).lower() for k, v in properties.items()}))

    for z in range(4):
        add(0, 0, z, 'stone')
        add(0, 1, z, 'redstone_wire', power=15 - min(z, 3 - z),
            north='side', east='none', south='side', west='none')
    for z in (0, 3):
        add(1, 1, z, 'stone')
        add(1, 0, z, 'redstone_wall_torch', facing='east', lit=False)
        add(-1, 0, z, 'lever', face='wall', facing='west', powered=True)
    return dict(
        schema_version='dustroute.mixed-piston-fixture.v1', id='device-torch-feedback',
        initial=dict(blocks=blocks), inputs=[position(-1, 0, z) for z in (0, 3)],
        steps=[dict(input=i, powered=p, wait_ticks=t) for i, p, t in
               [(0, False, 0), (1, False, 0), (0, True, 80), (1, True, 0),
                (0, False, 180), (1, False, 0), (0, True, 80), (1, True, 0)]],
        input_kinds=['lever', 'lever'], placement_mode='strict', warmup_ticks=100,
        settling_ticks=200, compare_drain_ticks=170, packet_input=True,
        align_first_input=True, shared_viewpoint=True,
        sample_full_region=True, require_vanilla_features=True,
        initial_hidden_state='strict unlit torches held off at new positions; empty history and queue',
    )


def locking_circuit(lock_first=False):
    blocks = []

    def add(x, y, z, name, **properties):
        blocks.append(dict(pos=position(x, y, z), name='minecraft:' + name,
                           properties={k: str(v).lower() for k, v in properties.items()}))

    for x, y, z in [(0, 1, 0), (1, 0, 0), (1, 0, 1), (1, 1, 2), (3, 2, 0)]:
        add(x, y, z, 'stone')
    add(-1, 1, 0, 'lever', face='wall', facing='west', powered=False)
    add(0, 1, 2, 'lever', face='wall', facing='west', powered=False)
    add(1, 1, 0, 'repeater', facing='west', delay=1, locked=False, powered=False)
    add(1, 1, 1, 'repeater', facing='south', delay=1, locked=False, powered=False)
    add(2, 1, 0, 'observer', facing='west', powered=False)
    add(3, 1, 0, 'sticky_piston', facing='up', extended=False)
    order = [1, 0] if lock_first else [0, 1]
    changes = [(i, True, 0) for i in order]
    changes += [(0, False, 1), (1, False, 1), (0, True, 1), (0, False, 1),
                (0, True, 12), (1, True, 4), (0, False, 4), (1, False, 4),
                (0, True, 1), (0, False, 1)]
    return dict(
        schema_version='dustroute.mixed-piston-fixture.v1',
        id='device-locking-' + ('lock-first' if lock_first else 'data-first'),
        initial=dict(blocks=blocks), inputs=[position(-1, 1, 0), position(0, 1, 2)],
        steps=[dict(input=i, powered=p, wait_ticks=t) for i, p, t in changes],
        input_kinds=['lever', 'lever'], placement_mode='strict', warmup_ticks=100,
        settling_ticks=80, packet_input=True, align_first_input=True,
        shared_viewpoint=True, sample_full_region=True, require_vanilla_features=True,
        initial_hidden_state='unpowered gates and observer; empty queues; fresh stationary piston',
    )


def separated_torch_feedback():
    # Two independent fast feedback loops exercise position-owned burnout in
    # the same queue. The connected variant alternates more slowly and alone
    # does not establish a sustained burnout interval.
    fixture = torch_feedback()
    fixture['id'] = 'device-torch-separated'
    fixture['initial']['blocks'] = [b for b in fixture['initial']['blocks'] if b['pos']['z'] in (0, 3)]
    for block in fixture['initial']['blocks']:
        if block['name'] == 'minecraft:redstone_wire':
            block['properties'].update({side: 'none' for side in DIRECTIONS})
    return fixture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--family', choices=['bulb', 'torch', 'torch-separated', 'locking'], default='bulb')
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    families = {'bulb': [bulb_circuit(False), bulb_circuit(True)],
                'torch': [torch_feedback()],
                'torch-separated': [separated_torch_feedback()],
                'locking': [locking_circuit(False), locking_circuit(True)]}[args.family]
    for base in families:
        for turns in range(4):
            fixture = rotate(base, turns)
            with (args.output_dir / (fixture['id'] + '.json')).open('x') as out:
                json.dump(fixture, out, indent=2)
                out.write('\n')


if __name__ == '__main__':
    main()
