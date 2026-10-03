#!/usr/bin/env python3
"""Capture a fixed four-block autonomous clock on the private Java test server.

Only server block-state queries supply observations. The default capture has no
external stimulus after construction; --diagnose-recovery marks its notification
explicitly and cannot be used as autonomous evidence. Retain the raw console log.
"""
import argparse
import hashlib
import json
import re
import socket
import subprocess
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True
from observe_torch_burnout import Server
from read_clock_block_ticks import read_ticks

JAR_SHA1 = '64bb6d763bed0a9f1d632ec347938594144943ed'
DIRECTIONS = ('north', 'east', 'south', 'west')


def snapshot(server, x, y, z):
    torch = f'{x + 1} {y} {z}'
    dust = f'{x} {y + 1} {z}'
    storage = 'dustroute:clock_probe'
    commands = [
        f'data modify storage {storage} sample set value {{power:-1,north:-1,east:-1,south:-1,west:-1}}',
        f'execute store result storage {storage} sample.tick long 1 run time query gametime',
        f'execute store success storage {storage} sample.lit byte 1 run execute if block {torch} minecraft:redstone_wall_torch[lit=true]',
        f'execute store success storage {storage} sample.torch byte 1 run execute if block {torch} minecraft:redstone_wall_torch[facing=east]',
        f'execute store success storage {storage} sample.support byte 1 run execute if block {x} {y} {z} minecraft:stone',
        f'execute store success storage {storage} sample.above byte 1 run execute if block {x + 1} {y + 1} {z} minecraft:stone',
    ]
    commands += [f'execute if block {dust} minecraft:redstone_wire[power={power}] run data modify storage {storage} sample.power set value {power}' for power in range(16)]
    commands += [f'execute if block {dust} minecraft:redstone_wire[{direction}={state}] run data modify storage {storage} sample.{direction} set value {index}'
                 for direction in DIRECTIONS for index, state in enumerate(('none', 'side', 'up'))]
    commands += [f'data get storage {storage} sample']
    lines = server.commands(commands)
    content = next(line for line in lines if 'contents: {' in line)

    def field(name):
        return int(re.search(r'\b' + name + r': (-?\d+)', content)[1])

    assert all(field(name) == 1 for name in ('torch', 'support', 'above')), content
    assert 0 <= field('power') <= 15, content
    assert all(0 <= field(direction) <= 2 for direction in DIRECTIONS), content
    return field('tick'), {
        'torch_lit': bool(field('lit')), 'torch_facing': 'east',
        'dust_power': field('power'),
        'dust_connections': {direction: ('none', 'side', 'up')[field(direction)] for direction in DIRECTIONS},
        'supports_intact': True,
    }


def capture(server, report, x, duration, directory, diagnose_recovery, checkpoints):
    y, z = 180, 23040
    region = f'{x - 2} {y - 2} {z - 2} {x + 3} {y + 3} {z + 2}'
    chunks = {(a // 16, b // 16) for a in range(x - 2, x + 4) for b in range(z - 2, z + 3)}
    for a, b in sorted(chunks):
        lines = server.commands([
            f'execute store success storage dustroute:clock_probe existing byte 1 run forceload query {a * 16} {b * 16}',
            'data get storage dustroute:clock_probe existing',
        ])
        assert any(line.endswith('contents: 0b') for line in lines), 'existing force-load ticket'
    server.commands([f'forceload add {x - 2} {z - 2} {x + 3} {z + 2}'])
    owned = False
    guard = ['data modify storage dustroute:clock_probe occupied set value 0b']
    guard += [f'execute unless loaded {a} {y} {b} run data modify storage dustroute:clock_probe occupied set value 1b' for a, b in [(a * 16, b * 16) for a, b in sorted(chunks)]]
    guard += [f'execute unless block {a} {b} {c} minecraft:air run data modify storage dustroute:clock_probe occupied set value 1b'
              for a in range(x - 2, x + 4) for b in range(y - 2, y + 4) for c in range(z - 2, z + 3)]
    guard += ['data get storage dustroute:clock_probe occupied']

    def queue_checkpoint(relative, stage):
        server.commands(['save-all flush'])
        queue_data = read_ticks(directory, x + 1, y, z, origin + relative)
        assert server.clock() == origin + relative, 'queue inspection advanced time'
        record = {'game_tick': relative, 'stage': stage, **queue_data}
        report.setdefault('queue_checkpoints', []).append(record)
        print(json.dumps(record), flush=True)

    try:
        server.advance(20)
        assert any(line.endswith('contents: 0b') for line in server.commands(guard)), 'guard occupied or unloaded'
        owned = True
        before = server.clock()
        server.commands([
            f'setblock {x} {y} {z} minecraft:stone',
            f'setblock {x + 1} {y + 1} {z} minecraft:stone',
            f'setblock {x} {y + 1} {z} minecraft:redstone_wire',
            f'setblock {x + 1} {y} {z} minecraft:redstone_wall_torch[facing=east,lit=true]',
        ])
        origin, sample = snapshot(server, x, y, z)
        assert origin == before, 'construction advanced game time'
        report['samples'].append({'game_tick': 0, **sample})
        print(json.dumps({'game_tick': 0, **sample}), flush=True)
        previous = origin
        for relative in range(1, duration + 1):
            server.commands(['tick step 1'])
            deadline = time.monotonic() + 5
            while True:
                tick, sample = snapshot(server, x, y, z)
                if tick == previous + 1:
                    break
                assert tick == previous, (previous, tick)
                if time.monotonic() >= deadline:
                    raise TimeoutError('one-game-tick step did not complete')
                time.sleep(0.01)
            previous = tick
            assert tick - origin == relative
            report['samples'].append({'game_tick': relative, **sample})
            if relative in checkpoints:
                queue_checkpoint(relative, 'after_tick')
            if diagnose_recovery and relative == 220:
                # Separate diagnostic only: this capture is not autonomous proof.
                notifier = f'{x + 1} {y} {z + 1}'
                server.commands([f'setblock {notifier} minecraft:glass', f'setblock {notifier} minecraft:air'])
                report['external_inputs'].append({'game_tick': relative, 'kind': 'adjacent_glass_place_remove_neighbor_notification'})
                queue_checkpoint(relative, 'after_notification')
            if relative % 100 == 0:
                print(json.dumps({'game_tick': relative, **sample}), flush=True)
        report['observation_complete'] = True
    finally:
        if owned:
            server.commands([f'fill {region} minecraft:air'])
            assert any(line.endswith('contents: 0b') for line in server.commands(guard)), 'cleanup failed'
            report['cleanup']['blocks_removed'] = True
        server.commands([f'forceload remove {x - 2} {z - 2} {x + 3} {z + 2}'])
        for a, b in sorted(chunks):
            lines = server.commands([
                f'execute store success storage dustroute:clock_probe existing byte 1 run forceload query {a * 16} {b * 16}',
                'data get storage dustroute:clock_probe existing',
            ])
            assert any(line.endswith('contents: 0b') for line in lines), 'ticket cleanup failed'
        report['cleanup']['tickets_removed'] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--x', type=int, required=True, help='Unused coordinate for this capture; y=180, z=23040')
    parser.add_argument('--ticks', type=int, default=640)
    parser.add_argument('--diagnose-recovery', action='store_true', help='Inspect saved block-tick queues and deliver one diagnostic neighbor notification at tick 220')
    parser.add_argument('--queue-checkpoint', action='append', type=int, default=[], help='Read saved block ticks at this game tick, reporting whether the chunk snapshot is current')
    args = parser.parse_args()
    assert 1 <= args.ticks <= 2000 and -29_000_000 <= args.x <= 29_000_000
    assert not args.diagnose_recovery or args.ticks >= 252
    checkpoints = set(args.queue_checkpoint)
    if args.diagnose_recovery:
        checkpoints.update((28, 30, 32, 190, 220, 222, 252))
    assert all(1 <= tick <= args.ticks for tick in checkpoints)
    directory = args.server_dir.resolve()
    assert re.search(r'^eula=true$', (directory / 'eula.txt').read_text(), re.M), 'existing EULA acceptance required'
    props = directory / 'server.properties'
    original = props.read_bytes()
    assert b'server-ip=127.0.0.1' in original and b'online-mode=false' in original
    port = int(re.search(rb'^server-port=(\d+)$', original, re.M)[1])
    with socket.socket() as sock:
        assert sock.connect_ex(('127.0.0.1', port)) != 0, 'server must already be stopped'
    jar = (directory / 'server.jar').read_bytes()
    assert hashlib.sha1(jar).hexdigest() == JAR_SHA1, 'unexpected server JAR'
    assert not args.output.exists(), 'observations are immutable'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    log = args.output.with_suffix('.console.log')
    assert not log.exists()
    report = {
        'schema': 'dustroute.periodic-clock-observation.v1', 'minecraft_version': '1.21.11',
        'evidence': 'observed_server_block_state', 'clock': 'game_tick',
        'sampling': 'frozen_console_after_each_explicit_tick',
        'construction': 'support_then_upper_block_then_dust_then_lit_wall_torch_in_one_frozen_tick',
        'external_inputs': [], 'internal_callback_order': 'unavailable',
        'placement': [
            {'position': {'x': 0, 'y': 0, 'z': 0}, 'state': 'minecraft:stone'},
            {'position': {'x': 1, 'y': 1, 'z': 0}, 'state': 'minecraft:stone'},
            {'position': {'x': 0, 'y': 1, 'z': 0}, 'state': 'minecraft:redstone_wire'},
            {'position': {'x': 1, 'y': 0, 'z': 0}, 'state': 'minecraft:redstone_wall_torch[facing=east,lit=true]'},
        ],
        'known_region': {'min': {'x': -2, 'y': -2, 'z': -2}, 'max': {'x': 3, 'y': 3, 'z': 2}},
        'duration_game_ticks': args.ticks, 'samples': [], 'observation_complete': False,
        'complete': False, 'cleanup': {}, 'server_jar_sha1': JAR_SHA1,
        'server_jar_sha256': hashlib.sha256(jar).hexdigest(),
        'capture_tool_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'console_helper_sha256': hashlib.sha256(Path(__file__).with_name('observe_torch_burnout.py').read_bytes()).hexdigest(),
    }
    if args.diagnose_recovery:
        report['diagnostic'] = 'saved_block_ticks_and_one_post_burnout_neighbor_notification'
    if checkpoints:
        report['saved_queue_checkpoint_ticks'] = sorted(checkpoints)
        report['queue_reader_sha256'] = hashlib.sha256(Path(__file__).with_name('read_clock_block_ticks.py').read_bytes()).hexdigest()
    server = None
    props.write_bytes(re.sub(rb'^pause-when-empty-seconds=.*$', b'pause-when-empty-seconds=0', original, flags=re.M))
    try:
        server = Server(directory, log)
        server.commands(['tick freeze'])
        capture(server, report, args.x, args.ticks, directory, args.diagnose_recovery, checkpoints)
    finally:
        try:
            if server:
                server.close()
                report['cleanup']['server_stopped'] = True
        finally:
            props.write_bytes(original)
            report['cleanup']['properties_restored'] = props.read_bytes() == original
            report['raw_console_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest() if log.exists() else None
            report['complete'] = report['observation_complete'] and all(report['cleanup'].get(key) is True for key in ('blocks_removed', 'tickets_removed', 'server_stopped', 'properties_restored'))
            args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'complete': report['complete'], 'samples': len(report['samples']), 'cleanup': report['cleanup']}), flush=True)


if __name__ == '__main__':
    main()
