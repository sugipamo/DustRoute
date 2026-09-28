"""Validate retained server confirmation envelopes, never infer missing state.

The source actor issues native block predicates. This checks the retained
receipt/data binding; it is not cryptographic attestation of a remote server.
"""
import hashlib
import json
import re


def confirmed_snapshot(client, label):
    sample = client['server_readbacks'][label]
    evidence = sample['readback']
    region = client['known_region']
    assert sample['min'] == region['min'] == evidence['min']
    assert sample['max'] == region['max'] == evidence['max']
    assert evidence['schema_version'] == 'dustroute.server-readback.v1'
    assert evidence['kind'] == 'server_confirmed'
    assert evidence['dimension'] == 'minecraft:overworld'
    assert evidence['request_id'] and re.fullmatch('[0-9a-f]{32}', evidence['nonce'])
    assert evidence['hidden_runtime_observed'] is False
    start, end = evidence['start_game_tick'], evidence['end_game_tick']
    assert type(start) is int and start >= 0 and start == end, 'readback crossed a server tick'
    volume = 1
    for a in ('x', 'y', 'z'):
        assert type(region['min'][a]) is int and type(region['max'][a]) is int
        width = region['max'][a] - region['min'][a] + 1
        assert width > 0
        volume *= width
    assert 1 <= volume <= 262144 and evidence['checked_cells'] == volume
    # Earlier captures enclosed predicates with native clock readings. New
    # captures retain the clock returned by each successful predicate itself.
    if 'predicate_ticks' in evidence:
        ticks = evidence['predicate_ticks']
        batch = evidence.get('predicate_batch_cells', 48)
        assert type(batch) is int and 1 <= batch <= 8880
        assert len(ticks) == (volume + batch - 1) // batch
        assert all(type(tick) is int and tick == start for tick in ticks)
    snapshot = {k: sample[k] for k in ('min', 'max', 'blocks')}
    digest = hashlib.sha256(json.dumps(snapshot, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
    assert digest == evidence['snapshot_sha256'], 'readback snapshot fingerprint differs'
    seen = set()
    for b in snapshot['blocks']:
        p = tuple(b['pos'][a] for a in ('x', 'y', 'z'))
        assert p not in seen and all(region['min'][a] <= b['pos'][a] <= region['max'][a] for a in ('x', 'y', 'z'))
        assert b['name'].startswith('minecraft:') and all(type(v) is str for v in b['properties'].values())
        seen.add(p)
    return snapshot
