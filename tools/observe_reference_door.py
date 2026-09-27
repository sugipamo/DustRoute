#!/usr/bin/env python3
"""Record the supplied door's live reference without claiming model conformance.

Strict placement initializes saved block states without construction callbacks.
Ordinary player interactions then drive the unmodified Java mechanism. Palette
commits and contiguous world heartbeats reconstruct every retained tick end.
The explicit interrupted mode records a single ON/OFF pulse without requiring
the aperture to recover; negative circuit outcomes still require complete data.
"""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import time

from compare_piston_transients import AIR, inside, parse_state, state, world_rows
from instrumented_server import InstrumentedServer, ensure_private_server
from observe_mixed_pistons import (
    ACTOR, ROOT, applied_inputs, digest, key, pos, require_post_world_inputs, save, snapshot,
)


def analyze(raw, client, fixture, *, interrupted=False):
    assert client['complete'] and client['placement_mode'] == 'strict'
    assert client['cleanup']['region_empty'] and client['cleanup']['force_load_removed']
    assert client['cleanup']['disconnected_before_finish'] is None
    assert raw[0]['heartbeat_mode'] == 'full'
    region = client['known_region']
    assert raw[0]['trace_bounds'] == ','.join(str(region[e][a]) for e in ('min', 'max') for a in ('x', 'y', 'z'))
    assert raw[-1]['kind'] == 'artifact_end' and raw[-1]['capture_state'] == 'closed'
    assert raw[-1]['write_errors'] == 0
    applied = applied_inputs(raw, client)
    require_post_world_inputs(raw, applied)
    assert [a['powered'] for a in applied] == ([True, False] if interrupted else [True, False, True, False])
    first, stop = applied[0]['game_tick'], applied[-1]['game_tick'] + 80
    if not interrupted:
        assert all(b['game_tick'] - a['game_tick'] >= 80 for a, b in zip(applied, applied[1:]))
    input_index = next(i for i, r in enumerate(raw) if r.get('sequence') == applied[0]['packet_sequence'])
    begin = max(i for i, r in enumerate(raw[:input_index]) if r.get('kind') == 'server_world_tick'
                and r.get('dimension') == 'minecraft:overworld' and r['phase'] == 'end')
    end = next(i for i, r in enumerate(raw) if i > begin and r.get('kind') == 'server_world_tick'
               and r.get('dimension') == 'minecraft:overworld' and r['phase'] == 'end' and r['game_tick'] == stop)
    window = raw[begin:end + 1]
    assert all(b['sequence'] == a['sequence'] + 1 for a, b in zip(window, window[1:])), 'capture gap'
    heartbeats = [(r['phase'], r['game_tick']) for r in window if r['kind'] == 'server_world_tick' and r.get('dimension') == 'minecraft:overworld']
    expected = [('end', first)]
    for tick in range(first, stop):
        expected.extend([('begin', tick), ('end', tick + 1)])
    assert heartbeats == expected, 'incomplete world tick boundaries'
    origin = client['origin']
    absolute = lambda p: tuple(p[a] + origin[a] for a in ('x', 'y', 'z'))
    initial = snapshot(client, 'initial')
    world = {key(b['pos']): state(b['name'], b['properties']) for b in initial['blocks']}
    wanted = {absolute(b['pos']): state(b['name'], b['properties']) for b in fixture['initial']['blocks']}
    assert world_rows(world) == world_rows(wanted), 'initial region differs from archive'
    ticks, commits, carriers = [], [], []
    for r in window:
        if r.get('dimension') != 'minecraft:overworld':
            continue
        p = key(pos(r['position'])) if 'position' in r else None
        if p is not None and not inside(p, region):
            continue
        if r['kind'] == 'state_commit':
            before, after = parse_state(r['before']), parse_state(r['after'])
            assert world.get(p, AIR) == before, f'unobserved write at {p}'
            world[p] = after
            commits.append({'tick':r['game_tick'] - first, 'position':p, 'before':before, 'after':after, 'flags':r['flags']})
        elif r['kind'] == 'carrier_trace':
            carriers.append({**{k:v for k,v in r.items() if k not in ('sequence', 'game_tick', 'dimension')}, 'tick':r['game_tick'] - first})
        elif r['kind'] == 'server_world_tick' and r['phase'] == 'end':
            ticks.append({'tick':r['game_tick'] - first, 'blocks':world_rows(world)})
    final = snapshot(client, 'final')
    assert world_rows(world) == world_rows({key(b['pos']):state(b['name'], b['properties']) for b in final['blocks']}), 'trace differs from final readback'
    aperture = {absolute(p) for p in fixture['aperture']}
    assert len(aperture) == 9 and all(inside(p, region) for p in aperture)
    assert all(wanted.get(p, AIR) == AIR for p in aperture), 'initial aperture is not clear'
    indexed = {t['tick']:dict(t['blocks']) for t in ticks}
    outcomes = []
    for index, action in enumerate(applied):
        if interrupted and index != len(applied) - 1:
            continue  # An interrupted ON is not promised time to finish.
        at = (applied[index + 1]['game_tick'] - first if index + 1 < len(applied) else stop - first)
        observed = indexed[at]
        cells = [observed.get(p, AIR) for p in sorted(aperture)]
        closed = all(b[0] == 'minecraft:smooth_quartz' for b in cells)
        opened = all(b == AIR for b in cells)
        if not interrupted:
            assert closed if action['powered'] else opened, f'aperture failed after input {index}'
        if not action['powered'] and not interrupted:
            assert world_rows(observed) == world_rows(wanted), 'open cycle did not restore complete initial region'
        outcomes.append({'input':index,'sample_tick':at,
                         'aperture':'closed' if closed else ('open' if opened else 'partial'),
                         'occupied_aperture':[{'position':p,'state':observed.get(p, AIR)} for p in sorted(aperture) if observed.get(p, AIR) != AIR],
                         'full_initial_region_restored':world_rows(observed) == world_rows(wanted)})
    assert carriers and commits
    return {
        'schema_version':'dustroute.reference-door-live.v1', 'minecraft_version':'1.21.11',
        'fixture':fixture['id'], 'initial':initial,
        'interrupted_input_probe':interrupted,
        'inputs':[{'position':a['position'],'powered':a['powered'],'tick':a['game_tick']-first} for a in applied],
        'tick_end_states':ticks, 'state_commits':commits, 'carrier_events':carriers,
        'outcomes':outcomes, 'cleanup':client['cleanup'],
        'evidence':{'contiguous_capture_window':True,'full_world_tick_heartbeats':True,
                    'whole_region_readback_matches':True,'model_conformance':False,
                    'initialization':'strict state placement followed by 100 client ticks of warmup; not public construction validation'},
    }


def main():
    capture_tool_sha256 = digest(Path(__file__))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--x', type=int, required=True)
    parser.add_argument('--interrupted', action='store_true',
                        help='record a single ON/OFF pulse, including incorrect settled aperture; never an adoption pass')
    args = parser.parse_args()
    assert args.run_id.replace('-', '').replace('_', '').isalnum()
    fixture = json.loads(args.fixture.read_text())
    assert fixture['schema_version'] == 'dustroute.mixed-piston-fixture.v1'
    assert fixture['placement_mode'] == 'strict' and fixture['warmup_ticks'] == 100
    assert 100 <= fixture['settling_ticks'] <= 200 and fixture['sample_full_region'] is True
    if args.interrupted:
        assert [s['powered'] for s in fixture['steps']] == [True, False]
        assert fixture['steps'][0]['wait_ticks'] == 0
        assert 1 <= fixture['steps'][1]['wait_ticks'] <= 200
    directory = Path('/root/DustRoute-minecraft-instrumentation')
    output = ROOT / '.local/e2e-artifacts'
    output.mkdir(parents=True, exist_ok=True)
    prefix = output / args.run_id
    paths = {s:prefix.with_suffix(s) for s in ('.raw.ndjson','.server.log','.client.json','.reference.json','.summary.json')}
    assert not any(p.exists() for p in paths.values())
    ensure_private_server(directory)
    origin = {'x':args.x,'y':180,'z':1000}
    bounds = [fixture['initial'][e][a] + origin[a] for e in ('min','max') for a in ('x','y','z')]
    print('Starting isolated target-version reference capture', flush=True)
    capture_ticks = sum(s['wait_ticks'] for s in fixture['steps']) + 120 if args.interrupted else 600
    assert 120 <= capture_ticks <= 600
    server = InstrumentedServer(directory, paths['.raw.ndjson'], paths['.server.log'], capture_ticks, bounds)
    try:
        env = {**os.environ, 'DUSTROUTE_MIXED_FIXTURE':str(args.fixture.resolve()),
               'DUSTROUTE_MIXED_OUTPUT':str(paths['.client.json']), 'DUSTROUTE_MIXED_X':str(args.x)}
        actor = subprocess.run(['node', str(ACTOR)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=190)
        print(actor.stdout, end='', flush=True)
        if actor.returncode:
            raise RuntimeError(actor.stderr or 'reference actor failed; inspect client artifact')
        time.sleep(8 if args.interrupted else 32)
    finally:
        server.close()
    raw = [json.loads(line) for line in paths['.raw.ndjson'].read_text().splitlines()]
    client = json.loads(paths['.client.json'].read_text())
    reference = analyze(raw, client, fixture, interrupted=args.interrupted)
    save(paths['.reference.json'], reference)
    summary = {'classification':'live_reference_recorded_not_model_verified',
               'fixture_sha256':digest(args.fixture), 'actor_sha256':digest(ACTOR),
               'capture_tool_sha256':capture_tool_sha256,
               'inputs':reference['inputs'],'outcomes':reference['outcomes'],
               'tick_end_states':len(reference['tick_end_states']),
               'state_commits':len(reference['state_commits']),
               'carrier_events':len(reference['carrier_events']),
               'committed_names':dict(Counter(c['after'][0] for c in reference['state_commits'])),
               'cleanup':client['cleanup'], 'raw_capture_end':raw[-1],
               'artifacts':{s:str(p) for s,p in paths.items()},
               'sha256':{s:digest(p) for s,p in paths.items() if p.exists()}}
    save(paths['.summary.json'], summary)
    print(json.dumps({k:v for k,v in summary.items() if k not in ('artifacts','sha256','raw_capture_end')}), flush=True)


if __name__ == '__main__':
    main()
