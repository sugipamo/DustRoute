#!/usr/bin/env python3
"""Compare scoped server callback boundaries with the electrical runtime trace.

Palette writes reconstruct the live world; staged model deltas reconstruct its
world. Callback projections and the ordered palette-write projection are checked
separately. No client sample supplies internal timing.
"""
import argparse
import json
from pathlib import Path
from observation_records import AIR, inside, key, parse_state, pos, state, world_rows
from observation_fixture import applied_inputs, require_post_world_inputs, snapshot

OBSERVED = {'minecraft:piston', 'minecraft:sticky_piston', 'minecraft:piston_head',
            'minecraft:moving_piston', 'minecraft:redstone_wire', 'minecraft:repeater'}


def model_state(block):
    if block['kind'] == 'Air':
        return AIR
    if block['kind'] == 'MovingPiston':
        entity = block.get('piston_entity')
        if entity is None:
            # Exact runtime write before the captured entity is registered.
            return state('minecraft:moving_piston', dict(facing=block['facing'].lower(), type=block['piston_variant']))
        payload = entity['pushed_block']
        # A moving source retains its body's variant; moved ordinary payloads
        # use the ordinary moving block, independently from the pushing body.
        variant = payload.get('piston_variant', 'normal') if entity['source'] else 'normal'
        return state('minecraft:moving_piston', dict(facing=entity['facing'].lower(), type=variant))
    assert block['observed_name'], f'unobserved model block: {block}'
    return state(block['observed_name'], block['observed_properties'])


def live_trace(raw, client, *, settle_ticks=8, epoch=1, verified_inputs=None,
               observed_names=OBSERVED, require_movement=True):
    assert raw[0]['transient_trace'] == 'dustroute.piston-transient.v1'
    assert raw[0]['heartbeat_mode'] == 'full', 'full world-tick heartbeats required'
    bounds = [client['known_region'][edge][axis] for edge in ('min', 'max') for axis in ('x', 'y', 'z')]
    assert raw[0]['trace_bounds'] == ','.join(map(str, bounds)), 'trace scope differs from observed region'
    assert client['complete'] and client['cleanup']['region_empty'] and client['cleanup']['force_load_removed']
    assert raw[-1]['kind'] == 'artifact_end' and raw[-1]['capture_state'] == 'closed'
    assert raw[-1]['write_errors'] == 0
    # Alternative ordinary controls must have independently verified their
    # applied palette writes before supplying this input record.
    applied = applied_inputs(raw, client) if verified_inputs is None else verified_inputs
    require_post_world_inputs(raw, applied)
    first = applied[0]['game_tick']
    start = applied[0]['packet_sequence']
    # The actor waits 20 client ticks before final readback; retain the bounded
    # mechanical drain before its cleanup. Deliberate cleanup is not input.
    last_input = applied[-1]['game_tick']
    stop = last_input + settle_ticks
    first_input_index = next(i for i,r in enumerate(raw) if r.get('sequence') == start)
    start_index = max(i for i,r in enumerate(raw[:first_input_index])
                      if r['kind'] == 'server_world_tick' and r.get('phase') == 'end'
                      and r.get('dimension') == 'minecraft:overworld')
    assert raw[start_index]['game_tick'] == first, 'missing preceding world-tick boundary'
    end_index = next(i for i,r in enumerate(raw) if i >= start_index
                     and r['kind'] == 'server_world_tick' and r.get('phase') == 'end'
                     and r.get('dimension') == 'minecraft:overworld' and r['game_tick'] == stop)
    window = raw[start_index:end_index+1]
    assert window and all(b['sequence'] == a['sequence'] + 1 for a, b in zip(window, window[1:])), 'gaps in declared trace window'
    boundaries = [(r['phase'], r['game_tick']) for r in window
                  if r['kind'] == 'server_world_tick' and r.get('dimension') == 'minecraft:overworld']
    # ServerWorld increments world time inside tick: HEAD sees the preceding
    # time, RETURN sees the new time. The initial boundary is post-world.
    expected_boundaries = [('end', first)]
    for tick in range(first, stop):
        expected_boundaries.extend([('begin', tick), ('end', tick + 1)])
    assert boundaries == expected_boundaries, 'incomplete world-tick heartbeat interval'
    world = {key(b['pos']): state(b['name'], b['properties']) for b in snapshot(client, 'initial')['blocks']}
    events = []
    # Use the actual post-world-tick input boundary. One positive origin is
    # shared by inputs, callbacks and savedWorldTime; fresh constructor zero
    # remains distinct from an observed carrier tick.
    world_phase = None
    for r in window:
        kind = r['kind']
        if kind == 'server_world_tick' and r.get('dimension') == 'minecraft:overworld':
            world_phase = r['phase']
        if kind == 'input_packet':
            assert world_phase == 'end', 'input applied inside a world tick is not covered by this replay'
        if r.get('dimension') != 'minecraft:overworld' or 'position' not in r:
            continue
        p = key(pos(r['position']))
        if not inside(p, client['known_region']):
            continue
        if kind not in {'state_commit', 'neighbor_update', 'piston_event', 'carrier_trace'}:
            continue
        tick = r['game_tick'] - first + epoch
        if kind == 'state_commit':
            before, after = parse_state(r['before']), parse_state(r['after'])
            assert world.get(p, AIR) == before, f'unobserved write before sequence {r["sequence"]}: {p}'
            world[p] = after
        elif kind == 'neighbor_update' and parse_state(r['target'])[0] in observed_names:
            assert world.get(p, AIR) == parse_state(r['target']), 'callback target differs from committed state'
            events.append(dict(tick=tick, kind='neighbor', position=p, world=world_rows(world)))
        elif kind == 'piston_event':
            events.append(dict(tick=tick, kind='event_'+r['stage'], position=p,
                               event=r['event_type'], world=world_rows(world)))
        elif kind == 'carrier_trace':
            carrier = None if r['removed'] else dict(progress=r['progress'], last_progress=r['last_progress'],
                saved_world_time=0 if r['saved_world_time'] == 0 else r['saved_world_time']-first+epoch,
                extending=r['extending'], source=r['source'], payload=parse_state(r['payload']))
            events.append(dict(tick=tick, kind=r['operation']+'_'+r['stage'], position=p,
                               carrier=carrier, world=world_rows(world)))
    final = {key(b['pos']): state(b['name'],b['properties']) for b in snapshot(client,'final')['blocks']}
    assert world_rows(world) == world_rows(final), 'drain world differs from final readback or includes cleanup'
    if require_movement:
        assert any(e['kind']=='event_begin' for e in events), 'no delivered piston event'
        assert any(e['kind']=='tick_begin' for e in events), 'no moving carrier evidence'
    return events, applied


def model_trace(initial, records, *, first_tick=1, observed_names=OBSERVED):
    world = {key(b['pos']): state(b['name'], b['properties']) for b in initial['blocks']}
    bodies = {}
    carriers = {}
    parents = {}
    previous_records = {}
    active = []
    events = []
    levels = {'Zero': 0.0, 'Half': 0.5, 'Full': 1.0}

    def carrier_at(position):
        if position not in carriers:
            return None
        history = carriers[position]['history']
        entity = bodies[position]['piston_entity']
        return dict(
            progress=levels[history['progress']],
            last_progress=levels[history['last_progress']],
            saved_world_time=history['saved_world_time'],
            extending=entity['extending'], source=entity['source'],
            payload=model_state(entity['pushed_block']),
        )

    def emit(scope, stage):
        event = dict(tick=scope['tick'], kind=scope['kind'] + '_' + stage,
                     position=scope['position'], world=world_rows(world))
        if scope['kind'] == 'event':
            event['event'] = scope['event']
        else:
            event['carrier'] = carrier_at(scope['position'])
        events.append(event)

    for record in records:
        invocation = record['invocation']
        parents[invocation['id']] = (invocation['cause'], invocation['root'])
        previous_records[invocation['id']] = record
        ancestors = set()
        parent = invocation['cause']
        while parent in parents and parents[parent][1] == invocation['root']:
            ancestors.add(parent)
            parent = parents[parent][0]
        while active and active[-1]['id'] not in ancestors:
            emit(active.pop(), 'end')
        if record['result'] != 'Executed':
            continue
        payload = invocation['call']['payload']
        position = key(invocation['call']['target'])
        tick = invocation['time']['game_tick']
        if tick >= first_tick:
            scope = None
            if isinstance(payload, dict) and 'Block' in payload:
                event_code = {'Extend': 0, 'Retract': 1, 'RetractDrop': 2}[payload['Block']['event']]
                scope = dict(kind='event', event=event_code)
            elif payload == 'CarrierTick':
                scope = dict(kind='tick')
            elif isinstance(payload, dict) and 'ForceFinish' in payload and position in carriers:
                scope = dict(kind='finish')
            if scope:
                scope.update(id=invocation['id'], tick=tick, position=position)
                emit(scope, 'begin')
                active.append(scope)
            parent_record = previous_records.get(invocation['cause'])
            parent_payload = None if parent_record is None else parent_record['invocation']['call']['payload']
            # World.updateComparators calls updateNeighbor directly, outside
            # the ordinary six-side Notify iterator. Count its actual device
            # delivery once; initialization/Added self checks remain excluded.
            if (isinstance(payload, dict) and payload.get('Device', {}).get('callback') == 'neighbor'
                    and isinstance(parent_payload, dict) and 'NotifyAnalogReaders' in parent_payload
                    and world.get(position, AIR)[0] in observed_names):
                events.append(dict(tick=tick, kind='neighbor', position=position, world=world_rows(world)))
            if isinstance(payload, dict) and 'Notify' in payload and payload['Notify']['jobs']:
                job = payload['Notify']['jobs'][0]
                target = key(job['target'])
                parent_record = previous_records.get(invocation['cause'])
                parent_payload = None if parent_record is None else parent_record['invocation']['call']['payload']
                # onBlockAdded calls tryMove directly. The runtime represents
                # this as a leading self job, not a vanilla neighborUpdate
                # callback. Retain the real trailing self callback and every
                # other ordinary notification in the declared projection.
                added_check = (
                    parent_record is not None and parent_record['delta'] is not None
                    and (parent_payload == 'CarrierTick'
                         or isinstance(parent_payload, dict) and 'ForceFinish' in parent_payload)
                    and target == key(parent_record['invocation']['call']['target'])
                    and any(change['after']['kind'] == 'Piston' and key(change['position']) == target
                            for change in parent_record['delta']['changes'])
                )
                if not added_check and not job['shape'] and world.get(target, AIR)[0] in observed_names:
                    events.append(dict(tick=tick, kind='neighbor', position=target, world=world_rows(world)))
        if record['delta']:
            for change in record['delta']['changes']:
                target = key(change['position'])
                world[target] = model_state(change['after'])
                bodies[target] = change['after']
        for target, _before, after in record['carrier_changes']:
            target = key(target)
            if after is None:
                carriers.pop(target, None)
            else:
                carriers[target] = after
    while active:
        emit(active.pop(), 'end')
    return events


def compare(live, model):
    # JSON also normalizes tuple/list transport differences.
    for index in range(max(len(live), len(model))):
        # Normalize one event at a time to avoid copying an entire mixed-circuit
        # world trace into a second JSON tree before locating its first mismatch.
        actual = json.loads(json.dumps(live[index])) if index < len(live) else None
        predicted = json.loads(json.dumps(model[index])) if index < len(model) else None
        if actual != predicted:
            return dict(index=index, live=actual, model=predicted,
                        live_count=len(live), model_count=len(model))
    return None


def live_writes(raw, client, applied, *, settle_ticks=8, epoch=1):
    """Palette writes in the already validated input/heartbeat window."""
    first = applied[0]
    return [dict(tick=r['game_tick'] - first['game_tick'] + epoch,
                 position=key(pos(r['position'])), before=parse_state(r['before']), after=parse_state(r['after']))
            for r in raw if r['kind'] == 'state_commit' and r.get('dimension') == 'minecraft:overworld'
            and inside(key(pos(r['position'])), client['known_region'])
            and r['sequence'] >= first['packet_sequence']
            and r['game_tick'] <= applied[-1]['game_tick'] + settle_ticks
            and parse_state(r['before']) != parse_state(r['after'])]


def model_writes(records):
    return [dict(tick=r['invocation']['time']['game_tick'], position=key(c['position']),
                 before=model_state(c['before']), after=model_state(c['after']))
            for r in records if r['delta'] for c in r['delta']['changes']
            if model_state(c['before']) != model_state(c['after'])]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--model-output', type=Path, help='explicitly selected replay output; original artifacts remain unchanged')
    parser.add_argument('--output', type=Path, help='new report path')
    args = parser.parse_args()
    prefix = args.prefix
    raw = [json.loads(line) for line in prefix.with_suffix('.raw.ndjson').read_text().splitlines()]
    client = json.loads(prefix.with_suffix('.client.json').read_text())
    model_path = args.model_output or prefix.with_suffix('.model.json')
    model = json.loads(model_path.read_text())
    live, applied = live_trace(raw, client)
    for record in model['trace']:
        invocation = record['invocation']
        payload = invocation['call']['payload']
        if isinstance(payload, dict) and 'Input' in payload:
            assert invocation['time']['section'] == 'after_world_tick', 'model replay used a different input boundary'
    predicted = model_trace(snapshot(client, 'initial'), model['trace'])
    report = dict(schema_version='dustroute.piston-transient-comparison.v2', applied_inputs=applied,
                  difference=compare(live, predicted), live=live, model=predicted)
    writes = live_writes(raw, client, applied)
    predicted_writes = model_writes(model['trace'])
    report['palette_writes'] = dict(live_count=len(writes), model_count=len(predicted_writes),
                                    difference=compare(writes, predicted_writes))
    destination = args.output or prefix.with_suffix('.transient.json')
    with destination.open('x') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    print(json.dumps(dict(output=str(destination), live_count=len(live), model_count=len(predicted),
                          first_difference=None if report['difference'] is None else report['difference']['index']), indent=2))
    return bool(report['difference'] or report['palette_writes']['difference'])


if __name__ == '__main__':
    raise SystemExit(main())
