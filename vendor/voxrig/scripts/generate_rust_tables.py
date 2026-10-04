#!/usr/bin/env python3
"""Deterministic Rust constants from pinned registry/oracle inputs, never runtime JSON.

Run normally to regenerate; --check compares without writing. No network, Cargo,
or build-time generation. Source JSON remains provenance and independent fixtures.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def source(path):
    data = (ROOT / 'data' / path).read_bytes()
    return json.loads(data), hashlib.sha256(data).hexdigest()

def text(value):
    # These pinned identifiers/property values are ASCII; JSON quoting is also Rust quoting.
    assert value.isascii() and all(ord(c) >= 32 for c in value)
    return json.dumps(value, ensure_ascii=False)

def optional(value, render):
    return 'None' if value is None else f'Some({render(value)})'

def number(value):
    assert math.isfinite(value)
    return repr(float(value))

def array(values, render=str):
    return '&[' + ', '.join(map(render, values)) + ']'

def property(p):
    count = p['num_values']
    assert count > 0
    values = p.get('values')
    assert values is None or len(values) == count
    kind = p['type']
    if kind == 'bool':
        assert count == 2 and values is None
        domain = 'PropertyValues::Boolean'
    elif kind == 'int':
        domain = f'PropertyValues::Integer {{ count: {count}, values: {optional(values, lambda v: array(v, text))} }}'
    elif kind == 'enum':
        assert values is not None
        domain = f'PropertyValues::Enum({array(values, text)})'
    else:
        raise ValueError(f'unknown property kind {kind}')
    return f'Property {{ name: {text(p["name"])}, values: {domain} }}'

def generate(version):
    prefix = '' if version == 'java_1_16_1' else version + '/'
    inputs = {}
    def load(path):
        data, digest = source(path)
        inputs[path] = digest
        return data
    blocks = load(prefix + 'blocks.json')
    items = load(prefix + 'items.json')
    lines = ['use crate::block_state::{BlockDefinition, Property, PropertyValues};',
             'use super::*;', '', 'pub(crate) static STATES: &[BlockDefinition] = &[']
    next_id = 0
    for b in blocks:
        assert b['minStateId'] == next_id
        assert math.prod(p['num_values'] for p in b['states']) == b['maxStateId'] - next_id + 1
        lines.append(f'    BlockDefinition {{ name: {text(b["name"])}, min_state_id: {next_id}, max_state_id: {b["maxStateId"]}, states: {array(b["states"], property)} }},')
        next_id = b['maxStateId'] + 1
    lines += ['];', '', 'pub(crate) static ITEMS: &[ItemDefinition] = &[']
    for item in items:
        assert 0 < item['stackSize'] <= 127
        lines.append(f'    ItemDefinition {{ id: {item["id"]}, name: {text(item["name"])}, stack_size: {item["stackSize"]} }},')
    lines += ['];', '']
    if version == 'java_1_16_1':
        lines.append('pub(crate) static MINING_BLOCKS: &[MiningBlock] = &[')
        for b in blocks:
            tools = b.get('harvestTools')
            tool_expr = optional(tools, lambda t: array(sorted(t.items(), key=lambda v: int(v[0])), lambda v: f'({int(v[0])}, {str(v[1]).lower()})'))
            lines.append(f'    MiningBlock {{ name: {text(b["name"])}, hardness: {optional(b.get("hardness"), number)}, min_state_id: {b["minStateId"]}, max_state_id: {b["maxStateId"]}, diggable: {str(b.get("diggable", False)).lower()}, material: {optional(b.get("material"), text)}, harvest_tools: {tool_expr} }},')
        lines += ['];', '', 'pub(crate) static MATERIALS: &[(&str, &[(i32, f64)])] = &[']
        for name, tools in sorted(load('materials.json').items()):
            lines.append(f'    ({text(name)}, {array(sorted(tools.items(), key=lambda v: int(v[0])), lambda v: f"({int(v[0])}, {number(v[1])})")}),')
        lines += ['];', '', 'pub(crate) static RECIPES: &[Recipe] = &[']
        for output, recipes in sorted(load('recipes.json').items(), key=lambda v: int(v[0])):
            for r in recipes:
                def grid(g):
                    return array(g, lambda row: array(row, lambda cell: optional(cell, str)))
                lines.append(f'    Recipe {{ output: {int(output)}, result: ({r["result"]["id"]}, {r["result"]["count"]}), ingredients: {optional(r.get("ingredients"), array)}, in_shape: {optional(r.get("inShape"), grid)}, out_shape: {optional(r.get("outShape"), grid)} }},')
        lines += ['];', '', 'pub(crate) static ENTITIES: &[(i32, &str, f64, f64)] = &[']
        for e in load('entities.json'):
            lines.append(f'    ({e["id"]}, {text(e["name"])}, {number(e["width"])}, {number(e["height"])}),')
        lines += ['];', '', 'pub(crate) static SOUNDS: &[(i32, &str)] = &[']
        for s in load('sounds.json'):
            lines.append(f'    ({s["id"]}, {text(s["name"])}),')
        lines += ['];', '']
        ranges = load('block_state_ranges.json')
        assert [(r['name'], r['minStateId'], r['maxStateId']) for r in ranges] == [(b['name'], b['minStateId'], b['maxStateId']) for b in blocks]
        data = load('block_collision_shapes.json')
        state_shapes = []
        for b in blocks:
            mapping = data['blocks'][b['name']]
            count = b['maxStateId'] - b['minStateId'] + 1
            if isinstance(mapping, int):
                state_shapes.extend([mapping] * count)
            else:
                assert len(mapping) >= count
                state_shapes.extend(mapping[:count])
        shape_ids = sorted(map(int, data['shapes']))
        assert shape_ids == list(range(len(shape_ids)))
        collision = {'state_shapes': state_shapes, 'shapes': [data['shapes'][str(i)] for i in shape_ids]}
    else:
        collision = load(prefix + 'collision_shapes.json')
    def shapes(name, data, outline=False):
        nonlocal lines
        assert len(data['state_shapes']) == next_id
        for index in data['state_shapes']:
            ids = [] if index is None else index if outline else [index]
            assert all(0 <= i < len(data['shapes']) for i in ids)
        lines.append(f'pub(crate) static {name}: {"OutlineShapes" if outline else "CollisionShapes"} = {"OutlineShapes" if outline else "CollisionShapes"} {{')
        lines.append('    state_shapes: &[')
        render = (lambda v: optional(v, lambda pair: '[' + ', '.join(map(str, pair)) + ']')) if outline else str
        for offset in range(0, next_id, 16):
            lines.append('        ' + ', '.join(render(v) for v in data['state_shapes'][offset:offset + 16]) + ',')
        lines += ['    ],', '    shapes: &[']
        for shape in data['shapes']:
            for box in shape:
                assert len(box) == 6 and all(math.isfinite(v) for v in box)
            lines.append('        ' + array(shape, lambda box: '[' + ', '.join(map(number, box)) + ']') + ',')
        lines += ['    ],']
        if outline:
            lines.append(f'    #[cfg(test)] registry_fnv64: {text(data["registry_fnv64"])},')
        lines += ['};', '']
    shapes('COLLISION', collision)
    if version == 'java_1_21_11':
        shapes('OUTLINE', load(prefix + 'outline_shapes.json'), True)
    header = ['// Generated by scripts/generate_rust_tables.py; do not edit.', '// Pinned input SHA-256:']
    header += [f'// {name}: {digest}' for name, digest in sorted(inputs.items())]
    return '\n'.join(header + [''] + lines)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for version in ['java_1_16_1', 'java_1_21_11']:
        path = ROOT / 'src' / 'tables' / f'{version}.rs'
        expected = generate(version)
        if args.check:
            if not path.is_file() or path.read_text() != expected:
                raise SystemExit(f'stale Rust tables: {path}')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expected)
    print('verified Rust tables' if args.check else 'generated Rust tables')

if __name__ == '__main__':
    main()
