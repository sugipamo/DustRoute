#!/usr/bin/env python3
"""Declare passive piston destruction trials; expectations come from the server."""
import argparse
import json

from pathlib import Path
from fixture_geometry import add, rotate
from make_support_loss_fixtures import circuit


def fixture(kind):
    f = circuit('wire')
    f['id'] = 'device-harvest-' + kind
    f['initial']['blocks'] = [b for b in f['initial']['blocks'] if b['pos']['x'] <= 0]
    f['initial']['blocks'][0]['name'] = 'minecraft:sticky_piston'
    f['steps'] = [dict(input=0, powered=True, wait_ticks=12),
                  dict(input=0, powered=False, wait_ticks=20)]
    f['require_server_readback'] = True
    if kind == 'branch':
        add(f, 1, 0, 0, 'slime_block')
        add(f, 1, 1, 0, 'stone')
        add(f, 2, 1, 0, 'pumpkin')
        add(f, 2, 0, 0, 'melon')
        add(f, 2, 1, 1, 'observer', facing='north', powered=False)
        add(f, 2, 1, 2, 'redstone_lamp', lit=False)
    elif kind == 'limit12':
        for x in range(1, 13):
            add(f, x, 0, 0, 'stone')
        add(f, 13, 0, 0, 'melon')
    elif kind == 'up':
        for b in f['initial']['blocks']:
            p = b['pos']
            p['x'], p['y'] = p['y'], p['x']
            if b['name'] == 'minecraft:sticky_piston':
                b['properties']['facing'] = 'up'
            elif b['name'] == 'minecraft:lever':
                b['properties'].update(face='floor', facing='north')
        for p in f['inputs']:
            p['x'], p['y'] = p['y'], p['x']
        add(f, 0, 1, 0, 'pumpkin')
    else:
        raise ValueError(kind)
    return rotate(f, 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for kind in ('branch', 'limit12', 'up'):
        f = fixture(kind)
        with (args.output_dir / (f['id'] + '.json')).open('x') as out:
            json.dump(f, out, indent=2)
            out.write('\n')


if __name__ == '__main__':
    main()
