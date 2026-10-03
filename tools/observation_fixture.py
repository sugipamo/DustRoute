"""Read retained observation fixtures; no client, server or live actor."""
from pathlib import Path
from observation_records import pos, key, properties

ROOT = Path(__file__).resolve().parents[1]

def snapshot(client, which):
    region = client["known_region"]
    rows = client[which]
    expected = 1
    for axis in ("x", "y", "z"):
        expected *= region["max"][axis] - region["min"][axis] + 1
    assert len(rows) == expected and len({key(b["position"]) for b in rows}) == expected
    assert all(all(region["min"][a] <= b["position"][a] <= region["max"][a] for a in ("x", "y", "z")) for b in rows)
    if 'server_readbacks' in client:
        from server_readback import confirmed_snapshot
        return confirmed_snapshot(client, which)
    return {**region, "blocks": [
        {"pos": b["position"], "name": b["name"], "properties": properties(b["properties"])}
        for b in rows if b["name"] != "minecraft:air"
    ]}


def applied_inputs(raw, client):
    packets = [r for r in raw if r.get("kind") == "input_packet"]
    assert len(packets) == len(client["activations"]), "input capture count differs"
    result = []
    for i, (packet, action) in enumerate(zip(packets, client["activations"])):
        assert pos(packet["position"]) == action["position"]
        end = packets[i + 1]["sequence"] if i + 1 < len(packets) else float("inf")
        level = str(action["requested_level"]).lower()
        candidates = [r for r in raw if r.get("kind") == "block_state_change"
                      and packet["sequence"] < r["sequence"] < end
                      and r.get("changed") and pos(r["position"]) == action["position"]
                      and "Block{minecraft:lever}" in r["before"]
                      and "Block{minecraft:lever}" in r["after"]
                      and f"powered={level}" in r["after"] and r["before"] != r["after"]]
        assert len(candidates) == 1, "cannot identify exactly one applied lever write"
        write = candidates[0]
        assert write["game_tick"] == packet["game_tick"], "application crossed packet boundary"
        result.append({"position": action["position"], "powered": action["requested_level"],
                       "game_tick": write["game_tick"], "packet_sequence": packet["sequence"],
                       "write_sequence": write["sequence"]})
    return result


def require_post_world_inputs(raw, applied):
    """Do not infer an input section from its timestamp or packet kind."""
    wanted = {row["packet_sequence"]: row["game_tick"] for row in applied}
    phase = None
    world_time = None
    seen = set()
    for row in raw:
        if row.get("dimension") != "minecraft:overworld":
            continue
        if row["kind"] == "server_world_tick":
            phase, world_time = row["phase"], row["game_tick"]
        sequence = row.get("sequence")
        if sequence in wanted:
            if phase != "end" or world_time != wanted[sequence]:
                raise ValueError("input is not evidenced at the post-world-tick boundary")
            seen.add(sequence)
    if seen != set(wanted):
        raise ValueError("missing applied input boundary")


