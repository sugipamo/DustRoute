#!/usr/bin/env python3
"""Compare every retained tick-end block state, preserving input tick sections.

This is an offline comparison, not a construction/adoption certificate. Carrier
histories and within-tick notification boundaries need separate comparisons.
"""
import argparse
import hashlib
import json
from pathlib import Path

from compare_piston_transients import AIR, model_state, state, live_trace, model_trace, live_writes, model_writes, compare as compare_events
from observe_mixed_pistons import key


def compare(reference, trial, model):
    assert reference['schema_version'] == 'dustroute.reference-door-observation.v1'
    assert trial['initial'] == reference['initial'], 'different initial conditions'
    assert trial['inputs'] == [dict(i, after_world_tick=True) for i in reference['inputs']], 'different input boundaries'
    assert model['status'] == {'kind': 'complete'} and model['pending'] == 0
    assert model['trace'], 'full model trace required'
    world = {key(b['pos']): state(b['name'], b['properties']) for b in trial['initial']['blocks']}
    changes = [(r['invocation']['time'], c) for r in model['trace'] if r['delta'] for c in r['delta']['changes']]
    sections = ['external', 'block_ticks', 'block_events', 'block_entities', 'after_world_tick']
    order = [(t['game_tick'], sections.index(t['section'])) for t, _ in changes]
    assert order == sorted(order), 'nonchronological model trace'
    ticks = reference['tick_end_worlds']
    assert [t['tick'] for t in ticks] == list(range(ticks[0]['tick'], ticks[-1]['tick'] + 1))
    assert ticks[0]['tick'] == 0
    index = 0
    differences = []
    for sample in ticks:
        tick = sample['tick']
        while index < len(changes) and order[index] < (tick, sections.index('after_world_tick')):
            _, change = changes[index]
            position = key(change['position'])
            assert world.get(position, AIR) == model_state(change['before']), 'model trace discontinuity'
            world[position] = model_state(change['after'])
            index += 1
        expected = {tuple(p): (b[0], tuple(map(tuple, b[1]))) for p, b in reference['world_states'][sample['world']]}
        cells = [{'position': p, 'live': expected.get(p, AIR), 'model': world.get(p, AIR)}
                 for p in sorted(expected.keys() | world.keys()) if expected.get(p, AIR) != world.get(p, AIR)]
        if cells:
            differences.append({'tick': tick, 'cells': cells})
    assert index == len(changes), 'model work outside retained live interval'
    return {'schema_version': 'dustroute.reference-door-comparison.v1', 'profile': model['profile'],
            'scope': 'all retained tick-end block identities/properties; not within-tick or carrier-history conformance',
            'classification': 'tick_end_mismatch' if differences else 'matched_tick_end_states',
            'ticks_compared': len(ticks), 'mismatched_ticks': len(differences),
            'first_mismatch': differences[0] if differences else None,
            'final_region_matches': not differences or differences[-1]['tick'] != ticks[-1]['tick'],
            'model_restoration_verified': model['restoration_verified'], 'differences': differences}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--raw', type=Path)
    parser.add_argument('--client', type=Path)
    parser.add_argument('--retain-callbacks', type=Path)
    args = parser.parse_args()
    files = {name: getattr(args, name) for name in ('reference', 'input', 'model')}
    reference, trial, model = (json.loads(files[name].read_text()) for name in files)
    result = compare(reference, trial, model)
    if args.raw or args.client or args.retain_callbacks:
        assert args.raw and args.client, 'callback comparison needs both raw and client capture'
        raw = [json.loads(line) for line in args.raw.read_text().splitlines()]
        client = json.loads(args.client.read_text())
        observed, applied = live_trace(raw, client, settle_ticks=reference['tick_end_worlds'][-1]['tick'] - reference['inputs'][-1]['tick'], epoch=0)
        assert [dict(tick=i['game_tick'] - applied[0]['game_tick'], position=i['position'], powered=i['powered']) for i in applied] == reference['inputs']
        # Initialization has no live counterpart: the capture's initial world
        # was already warmed up. Every subsequent root, including input at 0,
        # remains in this projection.
        initialization = {r['invocation']['root'] for r in model['trace'] if r['invocation']['call']['payload'] == 'Initialize'}
        predicted = model_trace(reference['initial'], [r for r in model['trace'] if r['invocation']['root'] not in initialization], first_tick=0)
        difference = compare_events(observed, predicted)
        result['callback_projection'] = dict(
            scope='piston events, carrier tick/finish boundaries and ordinary notifications to piston, head, moving piston, wire and repeater; complete callback-visible block states',
            observed_events=len(observed), model_events=len(predicted), first_mismatch=difference,
            classification='matched_callback_projection' if difference is None else 'callback_mismatch')
        files.update(raw=args.raw, client=args.client)
        writes = live_writes(raw, client, applied, settle_ticks=reference['tick_end_worlds'][-1]['tick'] - reference['inputs'][-1]['tick'], epoch=0)
        projected_writes = model_writes(model['trace'])
        result['palette_writes'] = dict(observed_writes=len(writes), model_writes=len(projected_writes), first_mismatch=compare_events(writes, projected_writes))
        if args.retain_callbacks:
            worlds = {}
            events = []
            for event in observed:
                world = event['world']
                digest = hashlib.sha256(json.dumps(world, separators=(',', ':')).encode()).hexdigest()
                worlds[digest] = world
                events.append(dict(event, world=digest))
            retained = dict(schema_version='dustroute.reference-door-callbacks.v1',
                reference_sha256=hashlib.sha256(args.reference.read_bytes()).hexdigest(),
                raw_sha256=hashlib.sha256(args.raw.read_bytes()).hexdigest(),
                client_sha256=hashlib.sha256(args.client.read_bytes()).hexdigest(),
                scope=result['callback_projection']['scope'], world_states=worlds, events=events)
            with args.retain_callbacks.open('x') as output:
                json.dump(retained, output, indent=2)
                output.write('\n')
    result['artifacts'] = {k: {'path': str(p.resolve()), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for k, p in files.items()}
    with args.output.open('x') as output:
        json.dump(result, output, indent=2)
        output.write('\n')
    print(json.dumps({k: result[k] for k in ('classification', 'ticks_compared', 'mismatched_ticks', 'final_region_matches')}))
    raise SystemExit(1 if result['differences'] or result.get('callback_projection', {}).get('first_mismatch') or result.get('palette_writes', {}).get('first_mismatch') else 0)


if __name__ == '__main__':
    main()
