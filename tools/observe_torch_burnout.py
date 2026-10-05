#!/usr/bin/env python3
"""Observe a local, stopped Java 1.21.11 test server through its console.

No client clock, inferred internal callbacks, EULA acceptance, or model output
is used as observation. Tick freezing and explicit one-game-tick steps make
each sample a server block-state observation. Raw console I/O is retained.
"""
import argparse
import hashlib
import json
import queue
import re
import subprocess
import threading
import time
from pathlib import Path


class Server:
    def __init__(self, directory, log, *, command=None, env=None,
                 force_kill_on_timeout=True):
        self.force_kill_on_timeout = force_kill_on_timeout
        self.log = log.open("x")
        self.lines = queue.Queue()
        self.fence = 0
        self.proc = subprocess.Popen(
            command or ["java", "-Xms512M", "-Xmx1024M", "-jar", "server.jar", "nogui"],
            cwd=directory, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, bufsize=1, env=env,
        )

        def consume():
            for line in self.proc.stdout:
                self.log.write(line)
                self.log.flush()
                self.lines.put(line.rstrip())
            self.lines.put(None)

        self.reader = threading.Thread(target=consume, daemon=True)
        self.reader.start()
        try:
            self.until(lambda line: 'Done (' in line, timeout=60)
        except BaseException:
            self.close()
            raise

    def until(self, predicate, timeout=10):
        deadline = time.monotonic() + timeout
        result = []
        while True:
            line = self.lines.get(timeout=max(0.001, deadline - time.monotonic()))
            if line is None:
                raise RuntimeError("server ended before command completed")
            result.append(line)
            if predicate(line):
                return result
            if time.monotonic() >= deadline:
                raise TimeoutError(result[-10:])

    def send(self, command):
        self.log.write('COMMAND ' + command + '\n')
        self.log.flush()
        self.proc.stdin.write(command + '\n')
        self.proc.stdin.flush()

    def commands(self, commands):
        self.fence += 1
        for command in commands:
            self.send(command)
        self.send(f'data modify storage dustroute:torch_probe fence set value {self.fence}')
        self.send('data get storage dustroute:torch_probe fence')
        ending = f'has the following contents: {self.fence}'
        return self.until(lambda line: line.endswith(ending))

    def state(self, pos, kind):
        lines = self.commands([
            'execute store result storage dustroute:torch_probe tick long 1 run time query gametime',
            f'execute store success storage dustroute:torch_probe lit byte 1 run execute if block {pos} {kind}[lit=true]',
            f'execute store success storage dustroute:torch_probe present byte 1 run execute if block {pos} {kind}',
            'data get storage dustroute:torch_probe',
        ])
        content = next(line for line in lines if 'contents: {' in line)
        assert re.search(r'present: 1b', content), content
        return int(re.search(r'tick: (\d+)L', content)[1]), bool(int(re.search(r'lit: ([01])b', content)[1]))

    def clock(self):
        lines = self.commands(['time query gametime'])
        return int(next(re.search(r'The time is (\d+)', line)[1] for line in lines if 'The time is ' in line))

    def advance(self, count):
        target = self.clock() + count
        self.commands([f'tick step {count}'])
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if self.clock() == target:
                return
            time.sleep(0.02)
        raise TimeoutError('setup tick step did not complete')

    def step(self, pos, kind, previous):
        self.commands(['tick step 1'])
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            tick, lit = self.state(pos, kind)
            if tick == previous + 1:
                return tick, lit
            assert tick == previous, (previous, tick)
            time.sleep(0.01)
        raise TimeoutError('tick step did not advance')

    def close(self):
        if self.proc.poll() is None:
            self.send('stop')
            try:
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                if not self.force_kill_on_timeout:
                    raise
                self.proc.kill()
                self.proc.wait(timeout=5)
        self.reader.join(timeout=5)
        self.log.close()
        if self.proc.returncode != 0:
            raise RuntimeError(f'server exited with status {self.proc.returncode}')


def schedules():
    # Every change is between completed game ticks, before the next tick.
    rapid = [(t, t % 4 == 0) for t in range(0, 32, 2)]
    window = [(t + delta, level) for t in [0, 8, 16, 24, 32, 40, 48]
              for delta, level in [(0, True), (2, False)]]
    return [
        ('seven_off_edges', rapid[:14], 190),
        ('eight_off_edges_hold_low', rapid, 195),
        ('eight_off_edges_hold_high', rapid[:-1], 195),
        ('cooldown_input_changes', rapid + [(70, True), (72, False)], 195),
        ('late_release_after_cooldown', rapid[:-1] + [(200, False)], 205),
        ('spaced_edges', [(t, t % 12 == 0) for t in range(0, 96, 6)], 100),
        ('second_burnout', rapid + [(t + 210, b) for t, b in rapid], 405),
        ('inclusive_sixty_tick_window', window + [(60, True), (62, False)], 225),
        ('expired_sixty_tick_window', window + [(61, True), (63, False)], 225),
    ]


def observe(server, name, changes, duration, standing, case_index):
    # A separate coordinate per case also separates Vanilla's position-keyed
    # burnout history and pending ticks; resetting a block alone would not.
    x, y, z = 1400 + 8 * case_index, 180, 1000
    support = f'{x} {y} {z}'
    source = f'{x - 1} {y} {z}'
    torch = f'{x} {y + 1} {z}' if standing else f'{x + 1} {y} {z}'
    notifier = f'{x} {y + 1} {z + 1}' if standing else f'{x + 1} {y} {z + 1}'
    kind = 'minecraft:redstone_torch' if standing else 'minecraft:redstone_wall_torch'
    state = '[lit=true]' if standing else '[facing=east,lit=true]'
    region = f'{x - 2} {y - 1} {z - 1} {x + 2} {y + 2} {z + 1}'
    # Do not remove an existing force-load ticket or overwrite nonempty space.
    for a in {(x - 2) // 16, (x + 2) // 16}:
        for b in {(z - 1) // 16, (z + 1) // 16}:
            existing = server.commands([
                f'execute store success storage dustroute:torch_probe existing byte 1 run forceload query {a * 16} {b * 16}',
                'data get storage dustroute:torch_probe existing',
            ])
            assert any(line.endswith('contents: 0b') for line in existing), 'existing force-load ticket'
    server.commands([f'forceload add {x - 2} {z - 1} {x + 2} {z + 1}'])
    owned = False
    try:
        server.advance(20)  # Allow asynchronously loaded chunks to become ticking.
        guard = ['data modify storage dustroute:torch_probe occupied set value 0b']
        guard += [f'execute unless loaded {a} {y} {z} run data modify storage dustroute:torch_probe occupied set value 1b'
                  for a in range(x - 2, x + 3)]
        guard += [f'execute unless block {a} {b} {c} minecraft:air run data modify storage dustroute:torch_probe occupied set value 1b'
                  for a in range(x - 2, x + 3) for b in range(y - 1, y + 3) for c in range(z - 1, z + 2)]
        guard += ['data get storage dustroute:torch_probe occupied']
        lines = server.commands(guard)
        assert any(line.endswith('contents: 0b') for line in lines), 'guard not empty or unloaded'
        owned = True
        server.commands([f'setblock {support} minecraft:stone',
                         f'setblock {source} minecraft:lever[face=wall,facing=west,powered=false]',
                         f'setblock {torch} {kind}{state}'])
        origin, lit = server.state(torch, kind)
        assert lit
        result = {'name': name, 'orientation': 'standing' if standing else 'wall',
                  'duration_game_ticks': duration, 'inputs': [],
                  'samples': [{'game_tick': 0, 'lit': lit}]}
        at = dict(changes)
        previous = origin
        for relative in range(duration):
            if relative in at:
                powered = at[relative]
                # /setblock is not a player lever interaction and does not call
                # LeverBlock's support-neighbor notification. Deliver a neutral
                # neighbor notification explicitly, within this frozen tick.
                server.commands([f'setblock {source} minecraft:lever[face=wall,facing=west,powered={str(powered).lower()}]',
                                 f'setblock {notifier} minecraft:glass', f'setblock {notifier} minecraft:air'])
                result['inputs'].append({'game_tick': relative, 'powered': powered})
                same, _ = server.state(torch, kind)
                assert same == previous, 'time advanced while frozen'
            previous, lit = server.step(torch, kind, previous)
            result['samples'].append({'game_tick': previous - origin, 'lit': lit})
        assert previous - origin == duration
        assert not result['samples'][2]['lit'], 'fixture did not invert the first powered input'
        return result
    finally:
        if owned:
            server.commands([f'fill {region} minecraft:air'])
            lines = server.commands(guard)
            assert any(line.endswith('contents: 0b') for line in lines), 'cleanup failed'
        server.commands([f'forceload remove {x - 2} {z - 1} {x + 2} {z + 1}'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', help='Run one named case only')
    args = parser.parse_args()
    directory = args.server_dir.resolve()
    assert re.search(r'^eula=true$', (directory / 'eula.txt').read_text(), re.M), 'existing EULA acceptance required'
    props = directory / 'server.properties'
    original = props.read_bytes()
    assert b'server-ip=127.0.0.1' in original and b'online-mode=false' in original, 'private local server required'
    assert not args.output.exists(), 'refusing to replace an observation'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    log = args.output.with_suffix('.console.log')
    assert not log.exists()
    assert hashlib.sha1((directory / 'server.jar').read_bytes()).hexdigest() == '64bb6d763bed0a9f1d632ec347938594144943ed', 'unexpected server JAR'
    # The server is launched by this process; restore its configuration on exit.
    updated = re.sub(rb'^pause-when-empty-seconds=.*$', b'pause-when-empty-seconds=0', original, flags=re.M)
    props.write_bytes(updated)
    server = None
    report = {'schema': 'dustroute.torch-observation.v1', 'minecraft_version': '1.21.11',
              'evidence': 'observed_server_block_state', 'clock': 'game_tick',
              'sampling': 'frozen_console_after_each_explicit_tick',
              'input_delivery': 'set_lever_state_then_glass_place_remove_neighbor_notification_in_same_frozen_tick',
              'server_jar_sha256': hashlib.sha256((directory / 'server.jar').read_bytes()).hexdigest(),
              'server_jar_sha1': hashlib.sha1((directory / 'server.jar').read_bytes()).hexdigest(),
              'internal_callback_order': 'unavailable', 'cases': [], 'complete': False}
    report['capture_tool_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    try:
        server = Server(directory, log)
        server.commands(['tick freeze'])
        for index, (name, changes, duration) in enumerate(schedules()):
            if args.case and name != args.case:
                continue
            for standing in (False, True):
                case = observe(server, name, changes, duration, standing, index * 2 + int(standing))
                report['cases'].append(case)
                edges = [s for a, s in zip(case['samples'], case['samples'][1:]) if a['lit'] != s['lit']]
                print(json.dumps({'name': name, 'orientation': case['orientation'], 'edges': edges}), flush=True)
        assert report['cases'], 'no matching cases'
        report['complete'] = True
    finally:
        try:
            if server:
                server.close()
        except BaseException:
            report['complete'] = False
            raise
        finally:
            props.write_bytes(original)
            report['raw_console_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest() if log.exists() else None
            args.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
