#!/usr/bin/env python3
"""Run the opt-in Blueprint lifecycle probe on the stopped private Vanilla server.

Only owned fixtures in the declared initially empty region are written. Independent
console predicates check all native snapshot cells at every checkpoint. Predicates
run in bounded batches and may span ticks; they are not an atomic observation.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess

from observe_torch_burnout import Server

ROOT = Path(__file__).resolve().parents[1]


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def confirm_snapshot(server, snapshot, number):
    expected = {}
    for block in snapshot["blocks"]:
        pos = tuple(block["pos"][a] for a in "xyz")
        assert pos not in expected, "duplicate snapshot coordinate"
        state = block["name"]
        assert re.fullmatch(r"minecraft:[a-z0-9_]+", state)
        properties = block.get("properties", {})
        assert all(re.fullmatch(r"[a-z0-9_]+", k) and re.fullmatch(r"[a-z0-9_.-]+", v)
                   for k, v in properties.items())
        if properties:
            state += "[" + ",".join(f"{k}={v}" for k, v in sorted(properties.items())) + "]"
        expected[pos] = state
    cells = [(x, y, z)
             for x in range(snapshot["min"]["x"], snapshot["max"]["x"] + 1)
             for y in range(snapshot["min"]["y"], snapshot["max"]["y"] + 1)
             for z in range(snapshot["min"]["z"], snapshot["max"]["z"] + 1)]
    assert set(expected).issubset(set(cells)), "block outside snapshot region"
    batches = []
    for start in range(0, len(cells), 100):
        group = cells[start:start + 100]
        marker = f"DUSTROUTE_BLUEPRINT_CHECK_{number}_{start}"
        predicates = " ".join(f"if block {x} {y} {z} {expected.get((x, y, z), 'minecraft:air')}"
                              for x, y, z in group)
        before = server.clock()
        lines = server.commands([f"execute {predicates} run say {marker}"])
        assert any(marker in line for line in lines), f"server/client mismatch in batch {start}"
        batches.append({"first_cell": start, "checked_cells": len(group),
                        "start_game_tick": before, "end_game_tick": server.clock(),
                        "all_predicates_passed": True})
    return {"kind": "independent_server_console_predicates", "checked_cells": len(cells),
            "snapshot_sha256": hashlib.sha256(json.dumps(snapshot, sort_keys=True,
                                                          separators=(",", ":")).encode()).hexdigest(),
            "atomic": False, "hidden_runtime_observed": False, "batches": batches}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--server-dir", type=Path, required=True)
    parser.add_argument("--allow-owned-fixture-writes", action="store_true")
    parser.add_argument("--recover-run-id", help="only plan fresh public removal of instances from this failed owned trial")
    args = parser.parse_args()
    assert args.allow_owned_fixture_writes, "explicit fixture-write opt-in required"
    assert re.fullmatch(r"[a-zA-Z0-9_-]+", args.run_id), "invalid run ID"
    directory = args.server_dir.resolve()
    props = (directory / "server.properties").read_text()
    for setting in ("server-ip=127.0.0.1", "server-port=25565", "online-mode=false",
                    "gamemode=creative", "allow-flight=true", "white-list=true"):
        assert setting in props.splitlines(), f"private test setting required: {setting}"
    assert "eula=true" in (directory / "eula.txt").read_text().splitlines()
    actors = {"DustRouteBot", "dustroutetest"}
    for name in ("whitelist.json", "ops.json"):
        assert actors <= {entry["name"] for entry in json.loads((directory / name).read_text())}
    assert hashlib.sha1((directory / "server.jar").read_bytes()).hexdigest() == "64bb6d763bed0a9f1d632ec347938594144943ed"
    try:
        with socket.create_connection(("127.0.0.1", 25565), timeout=1):
            raise AssertionError("server must be stopped; never restart a shared server")
    except ConnectionRefusedError:
        pass
    prefix = ROOT / ".local/e2e-artifacts" / args.run_id
    prefix.parent.mkdir(parents=True, exist_ok=True)
    trace = prefix.with_suffix(".jsonl")
    manifest_path = prefix.with_suffix(".manifest.json")
    assert not any(prefix.parent.glob(prefix.name + ".*")), "fresh run ID required"
    env = os.environ | {"DUSTROUTE_LIVE_BLUEPRINT_PROBE": "1", "MC_PORT": "25565",
                        "DUSTROUTE_STATE_DIR": str(prefix.with_suffix(".state")),
                        "TRACE_OUTPUT": str(trace), "PROBE_IDS_OUTPUT": str(prefix.with_suffix(".ids.json")),
                        "MCP_PROCESS_BIN": str(ROOT / "target/debug/dustroute-mcp")}
    if args.recover_run_id:
        assert re.fullmatch(r"[a-zA-Z0-9_-]+", args.recover_run_id)
        prior = prefix.parent / args.recover_run_id
        records = [json.loads(line) for line in prior.with_suffix(".jsonl").read_text().splitlines()]
        ids = [r["response"]["operation_id"] for r in records
               if r.get("stage") == "tool" and r["name"] == "new_placement"
               and r["args"].get("assembly_target") and r["response"].get("ok") is True]
        assert ids, "no owned placements to recover"
        env["PROBE_RECOVERY_INSTANCES"] = json.dumps(ids)
        external = {tuple(r["position"][a] for a in "xyz"): r for r in records
                    if r.get("stage") == "external_fixture_write"}
        env["PROBE_RECOVERY_EXTERNAL_WRITES"] = json.dumps(list(external.values()))
        env["DUSTROUTE_STATE_DIR"] = str(prior.with_suffix(".state"))
    manifest = {"schema_version": "dustroute.blueprint-iteration-live.v1", "run_id": args.run_id,
                "started_at_utc": datetime.now(timezone.utc).isoformat(),
                "recovery_from_run": args.recover_run_id,
                "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "server": "Vanilla Java 1.21.11", "endpoint": "127.0.0.1:25565", "dimension": "minecraft:overworld",
                "server_jar_sha256": fingerprint(directory / "server.jar"),
                "mcp_binary_sha256": fingerprint(ROOT / "target/debug/dustroute-mcp"),
                "probe_binary_sha256": fingerprint(ROOT / "target/debug/examples/blueprint_iteration_live"),
                "probe_source_sha256": fingerprint(ROOT / "crates/dustroute-mcp/examples/blueprint_iteration_live.rs"),
                "runner_source_sha256": fingerprint(Path(__file__)),
                "owned_region": {"min": {"x": 1280, "y": 179, "z": 1200}, "max": {"x": 1307, "y": 185, "z": 1207}},
                "backend": "Voxrig native", "actors": sorted(actors), "checkpoints": [], "passed": False,
                "limits": ["Finite declared cases, not an all-clear for arbitrary designs.",
                           "Runtime readbacks are client reconstructions; separate console predicates confirm stable checkpoints.",
                           "Independent predicate batches can span ticks; no atomic or hidden-queue guarantee.",
                           "Transient observer protection is a model refusal; the live comparison checks settled states.",
                           "Existing placed instances are not automatically upgraded."]}
    server = None
    probe = None
    try:
        server = Server(directory, prefix.with_suffix(".server.log"))
        probe = subprocess.Popen([str(ROOT / "target/debug/examples/blueprint_iteration_live")],
                                 cwd=ROOT, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, text=True, bufsize=1)
        with prefix.with_suffix(".probe.log").open("x") as log:
            for line in probe.stdout:
                log.write(line)
                log.flush()
                if line.startswith("POSITION_CLIENTS"):
                    server.commands(["tp DustRouteBot 1290.5 184 1204.5", "tp dustroutetest 1290.5 184 1204.5"])
                    probe.stdin.write("OK\n")
                    probe.stdin.flush()
                    print("POSITIONED test clients", flush=True)
                elif line.startswith("AIM_CLIENT "):
                    pos = json.loads(line.removeprefix("AIM_CLIENT "))
                    # Four-block reach, aimed at the selected fixture's north face.
                    server.commands([f"tp dustroutetest {pos['x'] + 0.5} {pos['y'] + 1} {pos['z'] + 2.5} 180 46.67"])
                    probe.stdin.write("OK\n")
                    probe.stdin.flush()
                    print("AIMED dummy at selected fixture", flush=True)
                elif line.startswith("CHECKPOINT "):
                    value = json.loads(line.removeprefix("CHECKPOINT "))
                    receipt = confirm_snapshot(server, value["snapshot"], len(manifest["checkpoints"]))
                    manifest["checkpoints"].append({"stage": value["stage"], "native_readback": value["readback"],
                                                     "independent_confirmation": receipt,
                                                     "non_air": sum(b["name"] != "minecraft:air" for b in value["snapshot"]["blocks"])})
                    probe.stdin.write("OK\n")
                    probe.stdin.flush()
                    print(f"CONFIRMED {value['stage']} ({receipt['checked_cells']} cells)", flush=True)
                else:
                    print(line.rstrip(), flush=True)
        manifest["probe_exit_code"] = probe.wait(timeout=15)
        records = [json.loads(line) for line in trace.read_text().splitlines()]
        manifest["passed"] = manifest["probe_exit_code"] == 0 and records[-1].get("passed") is True
        manifest["restored_to_air"] = records[-1].get("restored_to_air", False)
        manifest["tools"] = [{"name": r["name"], "blueprint_action": r["args"].get("blueprint", {}).get("action"),
                              "ok": r["response"]["ok"], "elapsed_ms": r["elapsed_ms"]}
                             for r in records if r.get("stage") == "tool"]
        manifest["refusals"] = [r for r in records if r.get("stage") == "tool" and r["response"]["ok"] is False]
        manifest["trace_sha256"] = fingerprint(trace)
        manifest["external_fixture_writes"] = [r for r in records if r.get("stage") == "external_fixture_write"]
        manifest["mcp_stops"] = [r for r in records if r.get("stage") == "mcp_stopped"]
        manifest["workflow_errors"] = [r["error"] for r in records
                                      if r.get("stage") == "workflow_result" and r.get("error")]
        assert manifest["passed"], "live trial failed; retained world and evidence require inspection"
    except BaseException as error:
        manifest["error"] = str(error)
        if probe is not None and probe.poll() is None:
            probe.stdin.close()
            try:
                manifest["probe_exit_code"] = probe.wait(timeout=20)
            except subprocess.TimeoutExpired:
                manifest["probe_did_not_exit"] = True
        raise
    finally:
        if server is not None:
            if server.proc.poll() is None:
                server.send("stop")
                server.proc.wait(timeout=30)
            server.reader.join(timeout=5)
            server.log.close()
            manifest["server_stopped_normally"] = server.proc.returncode == 0
        manifest["finished_at_utc"] = datetime.now(timezone.utc).isoformat()
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        print(f"EVIDENCE {manifest_path}", flush=True)


if __name__ == "__main__":
    main()
