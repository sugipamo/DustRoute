#!/usr/bin/env python3
"""Small adhesion trials. Expectations come from the independent server capture."""
import argparse
import json
from pathlib import Path

from fixture_geometry import add, rotate
from make_support_loss_fixtures import circuit


def fixture(kind):
    f = circuit('wire')
    f['id'] = 'device-adhesion-' + kind
    f['initial']['blocks'] = [b for b in f['initial']['blocks'] if b['pos']['x'] <= 0]
    f['initial']['blocks'][0]['name'] = 'minecraft:sticky_piston'
    f['steps'][1]['wait_ticks'] = 16
    f['require_server_readback'] = True
    if kind in ('blocked', 'limit13'):
        f['restoration_scope'] = 'input_notifications'
    name = 'honey_block' if kind.startswith('honey') else 'slime_block'
    if kind in ('limit12', 'limit13', 'merge'):
        count = {'limit12': 12, 'limit13': 13, 'merge': 4}[kind]
        for n in range(count):
            add(f, 1 + n % 2, n // 2, 0, name)
    else:
        add(f, 1, 0, 0, name)
        if kind == 'split':
            add(f, 1, 1, 0, 'honey_block')
            add(f, 1, 0, 1, 'obsidian')
        elif kind == 'support':
            add(f, 1, 1, 0, 'lever', face='floor', facing='north', powered=False)
        elif kind == 'observer':
            add(f, 1, 0, 1, 'observer', facing='north', powered=False)
            add(f, 2, 0, 2, 'redstone_lamp', lit=False)
        else:
            add(f, 1, 1, 0, name)
            add(f, 0, 1, 0, 'stone')
            if kind == 'blocked':
                add(f, 2, 1, 0, 'obsidian')
        if kind == 'rapid':
            f['steps'] = [dict(input=0, powered=p, wait_ticks=t) for p,t in
                          [(True,0),(False,1),(True,1),(False,16)]]
    if kind in ('honey-up', 'slime-down'):
        up = kind.endswith('up')
        sign = 1 if up else -1
        for b in f['initial']['blocks']:
            p = b['pos']
            p['x'], p['y'] = p['y'], sign * p['x']
            if b['name'] == 'minecraft:sticky_piston':
                b['properties']['facing'] = 'up' if up else 'down'
            elif b['name'] == 'minecraft:lever':
                b['properties'].update(face='floor' if up else 'ceiling', facing='north')
        for p in f['inputs']:
            p['x'], p['y'] = p['y'], sign * p['x']
    return f


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for kind, turns in [('slime',0),('honey-up',0),('slime-down',0),('merge',1),
                        ('split',0),('blocked',0),('support',0),('rapid',1),
                        ('limit12',0),('limit13',0),('observer',0)]:
        f = rotate(fixture(kind), turns)
        with (args.output_dir / (f['id']+'.json')).open('x') as stream:
            json.dump(f, stream, indent=2)
            stream.write('\n')


if __name__ == '__main__':
    main()
