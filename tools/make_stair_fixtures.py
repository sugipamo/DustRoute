#!/usr/bin/env python3
"""Declare dry stair trials; expected results are derived from the server."""
import argparse
import json
from pathlib import Path

from make_device_circuit_fixtures import position, rotate
from make_passive_shape_fixtures import add, step
from make_support_loss_fixtures import circuit


def stair(fixture, x, y, z, top, facing, shape='straight', name='stone_stairs'):
    add(fixture, x, y, z, name, half='top' if top else 'bottom', facing=facing,
        shape=shape, waterlogged=False)


def moving_corner(top=True, left=True, inner=True):
    fixture = circuit('wire')
    fixture['id'] = 'device-stairs-' + '-'.join(['inner' if inner else 'outer', 'top' if top else 'bottom', 'left' if left else 'right'])
    fixture['initial']['blocks'] = [b for b in fixture['initial']['blocks'] if b['pos']['x'] < 1]
    fixture['steps'][1]['wait_ticks'] = 16
    add(fixture, 1, 0, 0, 'stone')
    stair(fixture, 2, 0, 0, top, 'west' if left else 'east', name='cobblestone_stairs')
    z = -1 if inner else 1
    stair(fixture, 2, 0, z, top, 'north', ('inner_' if inner else 'outer_') + ('left' if left else 'right'), 'quartz_stairs')
    if inner:
        # Removing the rear partner changes inner -> straight, losing a side
        # face while the center stair remains at the same coordinate.
        x = 1 if left else 3
        add(fixture, x, 0, z, 'lever', face='wall', facing='west' if left else 'east', powered=False)
        add(fixture, x, 0, z - 1, 'observer', facing='south', powered=False)
        add(fixture, x, 0, z - 2, 'redstone_lamp', lit=False)
    else:
        add(fixture, 2, 1, z, 'observer', facing='down', powered=False)
        add(fixture, 2, 2, z, 'redstone_lamp', lit=False)
    return fixture


def arrival(rapid=False):
    fixture = circuit('wire')
    fixture['id'] = 'device-stairs-arrival-' + ('rapid-bottom-right' if rapid else 'top-left')
    fixture['initial']['blocks'] = [b for b in fixture['initial']['blocks'] if b['pos']['x'] < 1]
    if rapid:
        fixture['initial']['blocks'][0]['name'] = 'minecraft:sticky_piston'
    stair(fixture, 1, 0, 0, not rapid, 'north', name='smooth_quartz_stairs')
    stair(fixture, 2, 0, -1, not rapid, 'east' if rapid else 'west')
    add(fixture, 2, 1, 0, 'observer', facing='down', powered=False)
    add(fixture, 2, 2, 0, 'redstone_lamp', lit=False)
    fixture['steps'] = [dict(input=0, powered=p, wait_ticks=t) for p,t in
                        ([(True,0),(False,1),(True,1),(False,16)] if rapid else [(True,0),(False,16)])]
    return fixture


def wire_step(high_face):
    fixture = step('stone_slab', 'top')
    fixture['id'] = 'device-stairs-step-' + ('high-face' if high_face else 'low-face')
    for b in fixture['initial']['blocks']:
        if b['name'] == 'minecraft:stone_slab':
            b['name'] = 'minecraft:stone_stairs'
            b['properties'] = dict(half='top', facing='west' if high_face else 'east', shape='straight', waterlogged='false')
        if b['pos'] == position(-1,1,0):
            b['properties']['east'] = 'up' if high_face else 'side'
    return fixture


def cases():
    return [(moving_corner(top,left),turn) for top,left,turn in [(True,True,0),(True,False,1),(False,True,2),(False,False,3)]] + [
        (moving_corner(True,True,False),0), (moving_corner(False,False,False),1),
        (arrival(),0), (arrival(True),1), (wire_step(True),0), (wire_step(False),1)]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir',type=Path,required=True)
    args=parser.parse_args()
    args.output_dir.mkdir(parents=True,exist_ok=True)
    for fixture,turns in cases():
        fixture=rotate(fixture,turns)
        fixture['require_server_readback'] = True
        with (args.output_dir/(fixture['id']+'.json')).open('x') as f:
            json.dump(fixture,f,indent=2)
            f.write('\n')


if __name__ == '__main__':
    main()
