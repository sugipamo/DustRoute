"""Write a read-only vanilla diagnostic function for a retained client observation.

Use only with the isolated probe server. Run `reload`, then the printed function
from that server's console. Every cell/property, including air, is checked in one
function invocation. No placement or repair is performed by the function.
"""
import argparse
import json
import math
import re
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("capture", type=Path)
parser.add_argument("server", type=Path)
parser.add_argument("case")
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9_]{0,40}", args.case):
    raise SystemExit("invalid case identifier")
capture = json.loads(args.capture.read_text())
view = capture["after_client"]
if view["issue"] is not None:
    raise SystemExit("cannot confirm an incomplete reconstruction")
bounds = view["received"]["region"]
lengths = [bounds["max"][i] - bounds["min"][i] + 1 for i in range(3)]
if any(length <= 0 for length in lengths) or math.prod(lengths) > 1000:
    raise SystemExit("diagnostic is limited to 1000 cells")
expected = {
    (x, y, z)
    for x in range(bounds["min"][0], bounds["max"][0] + 1)
    for y in range(bounds["min"][1], bounds["max"][1] + 1)
    for z in range(bounds["min"][2], bounds["max"][2] + 1)
}
seen = set()
predicates = []
for block in view["blocks"]:
    position = tuple(block["position"])
    if position not in expected or position in seen or block["state"] is None:
        raise SystemExit("incomplete/duplicate/out-of-region cell")
    if block["moving"] is not None or block["state"]["name"] == "minecraft:moving_piston":
        raise SystemExit("final-state diagnostic cannot confirm moving block entities")
    seen.add(position)
    state = block["state"]
    name = state["name"]
    if not re.fullmatch(r"minecraft:[a-z0-9_]+", name):
        raise SystemExit("invalid block identifier")
    properties = []
    for key, value in sorted(state["properties"].items()):
        if not re.fullmatch(r"[a-z0-9_]+", key) or not re.fullmatch(r"[a-z0-9_]+", value):
            raise SystemExit("invalid property")
        properties.append(f"{key}={value}")
    if properties:
        name += "[" + ",".join(properties) + "]"
    predicates.append(f"execute unless block {' '.join(map(str, position))} {name} run return 0")
if seen != expected:
    raise SystemExit("missing cells")
pack = args.server / "isolated/datapacks/voxrig_observation_checks"
directory = pack / "data/voxrig/function"
directory.mkdir(parents=True, exist_ok=True)
(pack / "pack.mcmeta").write_text(json.dumps({"pack": {
    "description": "Isolated Voxrig observation assertions; no world mutations",
    "min_format": [94, 1], "max_format": [94, 1],
}}) + "\n")
command = "\n".join(predicates) + f"\nsay VOXRIG_MATCH_{args.case.upper()}_{len(expected)}\nreturn 1\n"
destination = directory / (args.case + ".mcfunction")
with destination.open("x") as output:
    output.write(command)
print(f"function voxrig:{args.case}")
