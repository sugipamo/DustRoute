"""Literal observation representation and artifact IO; no model or server lifecycle."""
import hashlib
import json
import re
AIR = ('minecraft:air', ())


def save(path, value):
    with path.open("x") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pos(raw):
    m = re.fullmatch(r"(?:BlockPos|Mutable)\{x=(-?\d+), y=(-?\d+), z=(-?\d+)\}", raw)
    if not m:
        raise ValueError(f"unrecognized server position {raw!r}")
    return dict(zip(("x", "y", "z"), map(int, m.groups())))


def key(position):
    return tuple(position[n] for n in ("x", "y", "z"))


def properties(values):
    return {k: str(v).lower() if isinstance(v, bool) else str(v) for k, v in values.items()}


def state(name, props):
    return name, tuple(sorted((k, str(v).lower() if isinstance(v, bool) else str(v)) for k, v in props.items()))


def parse_state(text):
    match = re.fullmatch(r'Block\{([^}]+)\}(?:\[([^]]*)\])?', text)
    if not match:
        raise ValueError(f'unsupported observed state {text!r}')
    return state(match[1], dict(item.split('=', 1) for item in (match[2] or '').split(',') if item))


def world_rows(world):
    return [(p, b) for p, b in sorted(world.items()) if b != AIR]


def inside(p, region):
    return all(region['min'][a] <= v <= region['max'][a] for a, v in zip(('x', 'y', 'z'), p))
