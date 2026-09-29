#!/usr/bin/env python3
"""Capture a mixed fixture in an isolated Java 1.21.11 region and replay writes.

The existing private instrumented server is required. Every run gets new output
files. Applied inputs are actual server lever state changes paired with received
packets; requested client delays never substitute for applied game ticks.
"""
import argparse
import json
import os
from pathlib import Path
from observation_records import save, digest, pos, key, properties
import re
import subprocess
import time

from instrumented_server import InstrumentedServer, ensure_private_server

ROOT = Path(__file__).resolve().parents[1]
ACTOR = ROOT / "crates/dustroute-mcp/mineflayer/e2e/piston-mixed-live.js"
MODEL = ROOT / "crates/dustroute-translate/examples/compare_electrical_pistons.rs"
WINDOW = 160


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--instrumentation-dir", type=Path, default=Path("/root/DustRoute-minecraft-instrumentation"))
    parser.add_argument("--fixture", type=Path, default=ROOT / "crates/dustroute-translate/tests/fixtures/mixed-electrical-independent-v1.json")
    parser.add_argument("--output-dir", type=Path, default=ROOT / ".local/e2e-artifacts")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--x", type=int, required=True)
    parser.add_argument("--capture-ticks", type=int, default=WINDOW)
    parser.add_argument("--transient-trace", action="store_true", help="record scoped commit, carrier and event evidence")
    args = parser.parse_args()
    assert 1 <= args.capture_ticks <= 1200, "bounded capture must be 1..1200 ticks"
    assert re.fullmatch(r"[A-Za-z0-9_-]+", args.run_id), "simple artifact name required"
    args.output_dir.mkdir(parents=True, exist_ok=True)
    args.output_dir = args.output_dir.resolve()
    args.fixture = args.fixture.resolve()
    prefix = args.output_dir / args.run_id
    paths = {suffix: prefix.with_suffix(suffix) for suffix in
             (".raw.ndjson", ".server.log", ".client.json", ".normalized.json", ".input.json", ".model.json", ".summary.json")}
    assert not any(p.exists() for p in paths.values()), "refusing to replace capture artifacts"
    fixture = json.loads(args.fixture.read_text())
    assert fixture["schema_version"] == "dustroute.mixed-piston-fixture.v1"
    ensure_private_server(args.instrumentation_dir)
    print("Starting isolated instrumented Java server", flush=True)
    trace_bounds = None
    if args.transient_trace:
        origin = dict(x=args.x, y=180, z=1000)
        trace_bounds = [fixture["initial"][edge][axis] + origin[axis]
                        for edge in ("min", "max") for axis in ("x", "y", "z")]
    server = InstrumentedServer(args.instrumentation_dir, paths[".raw.ndjson"], paths[".server.log"], args.capture_ticks, trace_bounds)
    try:
        env = os.environ.copy()
        env.update({"DUSTROUTE_MIXED_FIXTURE": str(args.fixture),
                    "DUSTROUTE_MIXED_OUTPUT": str(paths[".client.json"]),
                    "DUSTROUTE_MIXED_X": str(args.x)})
        actor = subprocess.run(["node", str(ACTOR)], cwd=ROOT, env=env, text=True, capture_output=True, timeout=190)
        print(actor.stdout, end="", flush=True)
        if actor.returncode:
            raise RuntimeError(actor.stderr)
        # Let the declared bounded observation window close before shutdown.
        time.sleep(args.capture_ticks / 20 + 2)
    finally:
        server.close()
    subprocess.run(["python3", str(args.instrumentation_dir / "scripts/normalize_ndjson.py"),
                    str(paths[".raw.ndjson"]), "-o", str(paths[".normalized.json"]),
                    "--scenario", args.run_id, "--dimension", "minecraft:overworld",
                    "--max-relative-ticks", str(args.capture_ticks)], cwd=ROOT, check=True)
    subprocess.run(["cargo", "run", "--offline", "--locked", "-j", "1", "-q", "-p", "dustroute-translate", "--example", "validate_vanilla_instrumentation",
                    "--", str(paths[".normalized.json"])], cwd=ROOT, check=True)
    client = json.loads(paths[".client.json"].read_text())
    raw = [json.loads(line) for line in paths[".raw.ndjson"].read_text().splitlines()]
    normalized = json.loads(paths[".normalized.json"].read_text())
    assert client["complete"] and client["cleanup"]["region_empty"]
    applied = applied_inputs(raw, client)
    require_post_world_inputs(raw, applied)
    first = applied[0]["game_tick"]
    initial = snapshot(client, "initial")
    final = snapshot(client, "final")
    save(paths[".input.json"], {"initial": initial, "trace": args.transient_trace, "inputs": [
        {"position": r["position"], "tick": r["game_tick"] - first + 1, "powered": r["powered"],
         "after_world_tick": True} for r in applied]})
    model = subprocess.run(["cargo", "run", "--offline", "--locked", "-j", "1", "-q", "-p", "dustroute-translate", "--example", "compare_electrical_pistons",
                            "--", str(paths[".input.json"])], cwd=ROOT, capture_output=True, text=True)
    differences = []
    if model.returncode:
        classification = "model_rejected"
        result = {"error": model.stderr}
    else:
        result = json.loads(model.stdout)
        expected = {key(b["position"]): (b["name"], b["properties"]) for b in result["blocks"] if b["name"] != "minecraft:air"}
        actual = {key(b["pos"]): (b["name"], b["properties"]) for b in final["blocks"]}
        for p in sorted(expected.keys() | actual.keys()):
            if expected.get(p) != actual.get(p):
                differences.append({"position": dict(zip(("x", "y", "z"), p)), "live": actual.get(p), "model": expected.get(p)})
        classification = "matched_final_state" if not differences else "model_state_mismatch"
    save(paths[".model.json"], result)
    summary = {"schema_version": "dustroute.mixed-piston-comparison.v1", "minecraft_version": "1.21.11",
               "fixture": fixture["id"], "fixture_sha256": digest(args.fixture),
               "actor_sha256": digest(ACTOR), "comparator_sha256": digest(MODEL),
               "applied_inputs": applied, "classification": classification, "differences": differences,
               "capture_state": raw[-1].get("capture_state"),
               "capture_sequence_contiguous": normalized["capture"]["sequence_contiguous"],
               "cleanup": client["cleanup"], "evidence_limit": "full settled state and applied inputs; no complete tick-internal conformance claim",
               "artifacts": {s: str(p) for s, p in paths.items()},
               "sha256": {s: digest(p) for s, p in paths.items() if p.exists()}}
    save(paths[".summary.json"], summary)
    print(json.dumps({"classification": classification, "summary": str(paths[".summary.json"]), "differences": len(differences)}), flush=True)
    if classification != "matched_final_state":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
