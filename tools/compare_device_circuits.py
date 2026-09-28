#!/usr/bin/env python3
"""Independent server-derived comparison for small mixed-device circuits.

No model result supplies an expected world or a missing input timestamp. The
projection covers full tick-end block states, palette-write order and delivered
block-tick order. Hidden torch history/comparator registers are not observed.
"""
from compare_piston_transients import AIR, OBSERVED, compare, inside, live_trace, model_trace, model_state, model_writes, parse_state, state, world_rows
from observe_mixed_pistons import key, pos, require_post_world_inputs, snapshot

DEVICE_OBSERVED = OBSERVED | {
    'minecraft:stone_button', 'minecraft:comparator', 'minecraft:observer',
    'minecraft:redstone_torch', 'minecraft:redstone_wall_torch', 'minecraft:redstone_lamp',
    'minecraft:waxed_copper_bulb', 'minecraft:waxed_exposed_copper_bulb',
    'minecraft:waxed_weathered_copper_bulb', 'minecraft:waxed_oxidized_copper_bulb',
}


class ReplayOutsideScope(AssertionError):
    """A recorded condition this bounded replay does not claim to model."""


def observe(raw, client, fixture):
    assert client['complete'] and client['cleanup']['region_empty'] and client['cleanup']['force_load_removed']
    assert client['cleanup']['disconnected_before_finish'] is None
    if fixture.get('require_server_readback'):
        from server_readback import confirmed_snapshot
        for label in ['initial', 'final', 'empty_before_setup', 'empty_after_cleanup']:
            verified = confirmed_snapshot(client, label)
            if label.startswith('empty_'):
                assert not verified['blocks'], 'server region not empty'
    if client['enabled_features'] != ['minecraft:vanilla'] or raw[0]['minecraft_version'] != '1.21.11':
        raise ReplayOutsideScope('requires Java 1.21.11 with only vanilla features')
    assert raw[0]['heartbeat_mode'] == 'full'
    region = client['known_region']
    assert raw[0]['trace_bounds'] == ','.join(str(region[e][a]) for e in ('min', 'max') for a in ('x', 'y', 'z'))
    assert raw[-1]['kind'] == 'artifact_end' and raw[-1]['capture_state'] == 'closed'
    assert raw[-1]['write_errors'] == 0
    packets = [r for r in raw if r['kind'] == 'input_packet']
    assert len(packets) == len(client['activations']) == len(fixture['steps']), 'input count differs'
    initial = snapshot(client, 'initial')
    initial_world = {key(b['pos']): state(b['name'], b['properties']) for b in initial['blocks']}
    origin = client['origin']
    expected = {tuple(b['pos'][a] + origin[a] for a in ('x', 'y', 'z')): state(b['name'], b['properties'])
                for b in fixture['initial']['blocks']}
    assert world_rows(initial_world) == world_rows(expected), 'initial region differs from declared fixture'
    applied = []
    for i, (packet, action, step) in enumerate(zip(packets, client['activations'], fixture['steps'])):
        p = pos(packet['position'])
        assert p == action['position']
        assert key(p) == tuple(fixture['inputs'][step['input']][a] + origin[a] for a in ('x', 'y', 'z'))
        kind = fixture['input_kinds'][step['input']]
        name = initial_world[key(p)][0]
        assert (kind, name) in [('use_device', 'minecraft:stone_button'), ('lever', 'minecraft:lever')]
        assert action['requested_level'] == step['powered']
        end = packets[i + 1]['sequence'] if i + 1 < len(packets) else float('inf')
        candidates = [r for r in raw if r['kind'] == 'state_commit'
                      and packet['sequence'] < r['sequence'] < end and pos(r['position']) == p
                      and parse_state(r['before'])[0] == parse_state(r['after'])[0] == name
                      and dict(parse_state(r['before'])[1])['powered'] != str(step['powered']).lower()
                      and dict(parse_state(r['after'])[1])['powered'] == str(step['powered']).lower()]
        assert len(candidates) == 1, f'input {i} does not have one verified application'
        write = candidates[0]
        assert packet['game_tick'] == write['game_tick'], 'application crossed receipt tick'
        assert kind != 'use_device' or step['powered'] is True
        applied.append(dict(position=p, game_tick=write['game_tick'], packet_sequence=packet['sequence'],
                            write_sequence=write['sequence'], kind=kind, powered=step['powered']))
    require_post_world_inputs(raw, applied)
    first = applied[0]['game_tick']
    stop = applied[-1]['game_tick'] + fixture.get('compare_drain_ticks', 40)
    if fixture.get('require_server_readback'):
        readbacks = client['server_readbacks']
        assert readbacks['initial']['readback']['end_game_tick'] <= first
        assert readbacks['final']['readback']['start_game_tick'] >= stop
        assert readbacks['empty_after_cleanup']['readback']['start_game_tick'] >= readbacks['final']['readback']['end_game_tick']
    packet_index = next(i for i, r in enumerate(raw) if r.get('sequence') == applied[0]['packet_sequence'])
    begin = max(i for i, r in enumerate(raw[:packet_index]) if r['kind'] == 'server_world_tick'
                and r['dimension'] == 'minecraft:overworld' and r['phase'] == 'end')
    end = next(i for i, r in enumerate(raw) if i > begin and r['kind'] == 'server_world_tick'
               and r['dimension'] == 'minecraft:overworld' and r['phase'] == 'end' and r['game_tick'] == stop)
    window = raw[begin:end + 1]
    assert all(b['sequence'] == a['sequence'] + 1 for a, b in zip(window, window[1:])), 'capture gap'
    beats = [(r['phase'], r['game_tick']) for r in window if r['kind'] == 'server_world_tick' and r['dimension'] == 'minecraft:overworld']
    wanted = [('end', first)]
    for tick in range(first, stop):
        wanted.extend([('begin', tick), ('end', tick + 1)])
    assert beats == wanted, 'missing tick boundaries'
    world = dict(initial_world)
    ticks, writes, scheduled, attempts = [], [], [], []
    for r in window:
        if r.get('dimension') != 'minecraft:overworld':
            continue
        p = key(pos(r['position'])) if 'position' in r else None
        if p is not None and not inside(p, region):
            continue
        if r['kind'] == 'state_commit':
            before, after = parse_state(r['before']), parse_state(r['after'])
            assert world.get(p, AIR) == before, 'unobserved mutation'
            world[p] = after
            writes.append(dict(tick=r['game_tick'] - first + 1, position=p, before=before, after=after))
        elif r['kind'] == 'server_world_tick' and r['phase'] == 'end':
            ticks.append(dict(tick=r['game_tick'] - first + 1, blocks=world_rows(world)))
        elif r['kind'] == 'ordered_tick' and r['scheduler'] == 'block':
            if r['trigger_game_tick'] != r['execution_game_tick']:
                raise ReplayOutsideScope('overdue tick outside this replay scope')
            delivered = world.get(p, AIR)[0] == r['type']
            attempts.append(dict(tick=r['execution_game_tick'] - first + 1, position=p, delivered=delivered))
            # The probe runs before ServerWorld.tickBlock's identity check.
            # With lifetime coverage enabled, compare attempts separately and
            # count only matching receivers as delivered device callbacks.
            if delivered or not fixture.get('compare_tick_attempts'):
                scheduled.append(dict(tick=r['execution_game_tick'] - first + 1, position=p, block=r['type']))
    final = {key(b['pos']): state(b['name'], b['properties']) for b in snapshot(client, 'final')['blocks']}
    assert world_rows(world) == world_rows(final), 'later work or cleanup differs from drain readback'
    inputs = [dict(tick=a['game_tick'] - first + 1, position=a['position'], after_world_tick=True,
                   **({'use_device': True} if a['kind'] == 'use_device' else {'powered': a['powered']})) for a in applied]
    callbacks, _ = live_trace(raw, client, settle_ticks=stop - applied[-1]['game_tick'],
                             verified_inputs=applied, observed_names=DEVICE_OBSERVED, require_movement=False)
    assert callbacks, 'no internal callback observations'
    worlds, world_ids, compact = [], {}, []
    for event in callbacks:
        world = tuple(event['world'])
        if world not in world_ids:
            world_ids[world] = len(worlds)
            worlds.append(event['world'])
        compact.append(dict(event, world=world_ids[world]))
    return dict(schema_version='dustroute.device-circuit-observation.v1', initial=initial, inputs=inputs,
                applied_inputs=applied, requested_steps=fixture['steps'], tick_end_worlds=ticks,
                writes=writes, scheduled_ticks=scheduled, cleanup=client['cleanup'],
                enabled_features=client['enabled_features'], hidden_state_observed=False,
                callback_worlds=worlds, callbacks=compact,
                **({'tick_attempts': attempts} if fixture.get('compare_tick_attempts') else {}))


def compare_model(observation, trial, model):
    assert observation['initial'] == trial['initial'] and observation['inputs'] == trial['inputs']
    assert model['status'] == {'kind': 'complete'} and model['pending'] == 0, 'incomplete model run'
    assert model['restoration_verified'] and model['trace']
    records = model['trace']
    initial_roots = {r['invocation']['root'] for r in records if r['invocation']['call']['payload'] == 'Initialize'}
    initialization = [r for r in records if r['invocation']['root'] in initial_roots]
    assert not model_writes(initialization), 'model initialization changed the observed stable initial world'
    records = [r for r in records if r['invocation']['root'] not in initial_roots]
    writes = model_writes(records)
    scheduled, attempts = [], []
    world = {key(b['pos']): state(b['name'], b['properties']) for b in trial['initial']['blocks']}
    events = []
    for r in records:
        inv = r['invocation']
        payload = inv['call']['payload']
        p = key(inv['call']['target'])
        if (inv['kind'] == 'scheduled_tick' and inv['time']['section'] == 'block_ticks'
                and isinstance(payload, dict) and payload.get('Device', {}).get('callback') == 'tick'):
            assert r['result'] in ['Executed', 'BlockReplaced']
            attempts.append(dict(tick=inv['time']['game_tick'], position=p, delivered=r['result'] == 'Executed'))
            if r['result'] == 'Executed':
                scheduled.append(dict(tick=inv['time']['game_tick'], position=p, block=world.get(p, AIR)[0]))
        for c in (r.get('delta') or {}).get('changes', []):
            events.append((inv['time'], c))
            assert world.get(key(c['position']), AIR) == model_state(c['before']), 'model trace gap'
            world[key(c['position'])] = model_state(c['after'])
    world = {key(b['pos']): state(b['name'], b['properties']) for b in trial['initial']['blocks']}
    index, tick_differences = 0, []
    sections = ['external', 'block_ticks', 'block_events', 'block_entities', 'after_world_tick']
    ordering = [(t['game_tick'], sections.index(t['section'])) for t, _ in events]
    assert ordering == sorted(ordering), 'nonchronological model'
    for sample in observation['tick_end_worlds']:
        while index < len(events) and ordering[index] < (sample['tick'], 4):
            c = events[index][1]
            world[key(c['position'])] = model_state(c['after'])
            index += 1
        difference = compare(sample['blocks'], world_rows(world))
        if difference:
            tick_differences.append(dict(tick=sample['tick'], first_difference=difference))
    assert index == len(events), 'model work extends past live window'
    actual_callbacks = [dict(e, world=observation['callback_worlds'][e['world']]) for e in observation['callbacks']]
    predicted_callbacks = model_trace(trial['initial'], records, observed_names=DEVICE_OBSERVED)
    return dict(schema_version='dustroute.device-circuit-comparison.v1', profile=model['profile'],
                tick_end_count=len(observation['tick_end_worlds']), tick_mismatches=tick_differences,
                live_write_count=len(observation['writes']), model_write_count=len(writes),
                first_write_mismatch=compare(observation['writes'], writes),
                live_scheduled_count=len(observation['scheduled_ticks']), model_scheduled_count=len(scheduled),
                first_scheduled_mismatch=compare(observation['scheduled_ticks'], scheduled),
                first_tick_attempt_mismatch=compare(observation['tick_attempts'], attempts) if 'tick_attempts' in observation else None,
                live_callback_count=len(actual_callbacks), model_callback_count=len(predicted_callbacks),
                first_callback_mismatch=compare(actual_callbacks, predicted_callbacks),
                restoration_verified=True,
                scope='complete tick-end block states, palette writes, delivered device block ticks, active-device ordinary notifications and piston event/carrier boundaries; hidden registers/histories, shape and inert-target callbacks not compared')
