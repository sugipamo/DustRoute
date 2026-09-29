#!/usr/bin/env python3
"""Explicit replacement candidate for an existing mixed Assembly trial."""
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
    names = {old['candidate_parent']: 'runtime-test.adhesion-parent.v1',
             old['next_child']: 'runtime-test.adhesion-mechanism.v1',
             old['candidate_state']['id']: 'runtime-test.adhesion-state.v1'}

    def rename(value):
        if isinstance(value, dict):
            return {k: rename(v) for k,v in value.items()}
        if isinstance(value, list):
            return [rename(v) for v in value]
        return names.get(value, value) if isinstance(value, str) else value

    request = rename(old)
    mechanism, = [r for r in request['revisions'] if r['id'] == request['next_child']]
    payload, = [b for b in mechanism['blocks'] if b['position'] == dict(x=0,y=2,z=0)]
    payload['block'].update(kind='Transparent', observed_name='minecraft:slime_block',
                            observation_classification='exact', observed_properties={})
    wing, = [b for b in mechanism['blocks'] if b['position'] == dict(x=0,y=2,z=-1)]
    wing['block'].update(kind='Solid', observed_name='minecraft:stone',
                         observation_classification='exact', observed_properties={})
    honey = copy.deepcopy(payload)
    honey['position']['x'] = 1
    honey['block']['observed_name'] = 'minecraft:honey_block'
    mechanism['blocks'].append(honey)
    request['candidate_state']['assembly']['blocks'] = copy.deepcopy(mechanism['blocks'])
    request['title'] = 'Mixed piston Assembly with slime adhesion and honey separation'
    request['description'] = 'An upward sticky piston moves slime and its stone side branch. Adjacent honey remains separate.'
    fixture['request'] = request
    fixture.pop('require_stair_readback_correction', None)
    with args.output.open('x') as output:
        json.dump(fixture, output, indent=2)
        output.write('\n')


if __name__ == '__main__':
    main()
