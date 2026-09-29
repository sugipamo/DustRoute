"""Fixture geometry only; never executes a model or starts a server."""
import json
DIRECTIONS = ('north', 'east', 'south', 'west')


def position(x, y, z):
    return dict(x=x, y=y, z=z)


def rotate(fixture, turns):
    fixture = json.loads(json.dumps(fixture))

    def pos(p):
        x, z = p['x'], p['z']
        for _ in range(turns):
            x, z = -z, x
        return position(x, p['y'], z)

    def side(s):
        return DIRECTIONS[(DIRECTIONS.index(s) + turns) % 4] if s in DIRECTIONS else s

    for b in fixture['initial']['blocks']:
        b['pos'] = pos(b['pos'])
        b['properties'] = {side(k): side(v) if k == 'facing' else v for k, v in b['properties'].items()}
    if fixture.get('shared_viewpoint'):
        first = fixture['inputs'][0]
        fixture['viewpoint'] = pos(position(first['x'] + 0.5, first['y'], first['z'] + 2.5))
    fixture['inputs'] = [pos(p) for p in fixture['inputs']]
    for edge, fn, offset in [('min', min, -3), ('max', max, 3)]:
        fixture['initial'][edge] = {a: fn(b['pos'][a] for b in fixture['initial']['blocks']) + offset for a in ('x', 'y', 'z')}
    fixture['id'] += f'-r{90 * turns}'
    fixture['rotation'] = 90 * turns
    return fixture


def add(fixture, x, y, z, name, **properties):
    fixture['initial']['blocks'].append(dict(pos=position(x, y, z), name='minecraft:' + name,
                                           properties={k: str(v).lower() for k, v in properties.items()}))
