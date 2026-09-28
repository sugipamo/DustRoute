#!/usr/bin/env python3
"""Explicit candidate revision for stair motion in the public Assembly workflow."""
import argparse
import copy
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    fixture = json.loads(args.source.read_text())
    old = fixture['request']
    names = {
        old['candidate_parent']: 'runtime-test.stair-parent.v1',
        old['next_child']: 'runtime-test.stair-mechanism.v1',
        old['candidate_state']['id']: 'runtime-test.stair-state.v1',
    }

    def rename(value):
        if isinstance(value, dict):
            return {k: rename(v) for k, v in value.items()}
        if isinstance(value, list):
            return [rename(v) for v in value]
        return names.get(value, value) if isinstance(value, str) else value

    request = rename(old)
    mechanism, = [r for r in request['revisions'] if r['id'] == request['next_child']]
    payload, = [b for b in mechanism['blocks'] if b['position'] == dict(x=0, y=2, z=0)]
    payload['block'].update(kind='Transparent', observed_name='minecraft:cobblestone_stairs',
                            observation_classification='exact', observed_properties=dict(
                                facing='west', half='top', shape='straight', waterlogged='false'))
    neighbor = copy.deepcopy(payload)
    neighbor['position']['z'] = -1
    neighbor['block']['observed_name'] = 'minecraft:quartz_stairs'
    neighbor['block']['observed_properties'].update(facing='north', shape='inner_left')
    mechanism['blocks'].append(neighbor)
    request['candidate_state']['assembly']['blocks'] = copy.deepcopy(mechanism['blocks'])
    request['title'] = 'Mixed piston Assembly with independently confirmed stair shapes'
    request['description'] = 'The upward sticky piston moves the rear stair partner. The stationary stair becomes straight while extended and regains its inner corner after retraction.'
    fixture['request'] = request
    fixture['require_stair_readback_correction'] = True
    with args.output.open('x') as output:
        json.dump(fixture, output, indent=2)
        output.write('\n')


if __name__ == '__main__':
    main()
