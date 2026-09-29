#!/usr/bin/env python3
"""Run the custom-Assembly public MCP workflow on the isolated Java server."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import time

from observe_mixed_pistons import ROOT
from observation_records import digest, save
import instrumented_server
from instrumented_server import InstrumentedServer, ensure_private_server


def status():
    with socket.create_connection(("127.0.0.1", 25580), timeout=2) as connection:
        connection.sendall(b'{"id":1,"method":"status","params":{}}\n')
        return json.loads(connection.makefile().readline())["result"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--x", required=True, type=int)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--rotation", choices=("r0", "r90", "r180", "r270"), default="r90")
    parser.add_argument("--persistence", action="store_true", help="restart after placement and verify durable observation/removal")
    parser.add_argument("--capture-construction", action="store_true", help="retain continuous server events for the bounded construction region")
    parser.add_argument("--instrumentation-dir", type=Path, default=Path("/root/DustRoute-minecraft-instrumentation"))
    args = parser.parse_args()
    import re
    assert re.fullmatch(r"[A-Za-z0-9_-]+", args.run_id)
    prefix = ROOT / ".local/e2e-artifacts" / args.run_id
    paths = {s: prefix.with_suffix(s) for s in (".raw.ndjson", ".server.log", ".bridge.log", ".client.json", ".fixture.json", ".summary.json")}
    assert not any(p.exists() for p in paths.values()), "new artifact prefix required"
    state_dir = prefix.with_suffix(".state")
    assert not state_dir.exists(), "new isolated MCP state directory required"
    ensure_private_server(args.instrumentation_dir)
    try:
        with socket.create_connection(("127.0.0.1", 25580), timeout=1):
            raise AssertionError("bridge port already in use")
    except (ConnectionRefusedError, TimeoutError):
        pass
    fixture = json.loads(args.fixture.read_text())
    save(paths[".fixture.json"], fixture)
    region = fixture["request"]["behavior_context"]["known_region"]
    def transformed(p):
        x, z = {"r0": (p['x'], p['z']), "r90": (-p['z'], p['x']),
                "r180": (-p['x'], -p['z']), "r270": (p['z'], -p['x'])}[args.rotation]
        return x + args.x, p['y'] + 180, z + 1000
    a, b = transformed(region['min']), transformed(region['max'])
    trace_bounds = [min(x, y) for x, y in zip(a, b)] + [max(x, y) for x, y in zip(a, b)]
    actor = ROOT / "crates/dustroute-mcp/mineflayer/e2e/piston-assembly-mcp-live.js"
    # Record the sources actually loaded by this run before starting processes.
    source_hashes = {str(p): digest(p) for p in (
        Path(__file__).resolve(), actor, ROOT / "target/debug/dustroute-mcp",
        ROOT / "crates/dustroute-mcp/mineflayer/bridge.js",
        ROOT / "crates/dustroute-mcp/mineflayer/readback.js",
        Path(instrumented_server.__file__).resolve(),
    )}
    env = os.environ.copy()
    env.update({"DUSTROUTE_SERVER_ADDRESS": "127.0.0.1:25565", "DUSTROUTE_STATE_DIR": str(state_dir),
                "DUSTROUTE_ASSEMBLY_FIXTURE": str(paths[".fixture.json"]), "DUSTROUTE_ASSEMBLY_OUTPUT": str(paths[".client.json"]),
                "DUSTROUTE_ASSEMBLY_X": str(args.x), "DUSTROUTE_ASSEMBLY_ROTATION": args.rotation, "DUSTROUTE_MC_AUTH": "offline",
                "DUSTROUTE_ASSEMBLY_PERSISTENCE": str(args.persistence).lower()})
    server = InstrumentedServer(args.instrumentation_dir, paths[".raw.ndjson"], paths[".server.log"],
                                capture_ticks=1200 if args.persistence else 160,
                                trace_bounds=trace_bounds,
                                capture_mode="continuous" if args.capture_construction else "bounded")
    bridge = None
    observed_status = None
    try:
        with paths[".bridge.log"].open("x") as output:
            bridge = subprocess.Popen(["node", str(ROOT / "crates/dustroute-mcp/mineflayer/bridge.js")], cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
            for _ in range(120):
                if bridge.poll() is not None:
                    raise RuntimeError("test bridge stopped before joining")
                try:
                    observed_status = status()
                    if observed_status["connected"]:
                        break
                except (OSError, KeyError, json.JSONDecodeError):
                    pass
                time.sleep(0.5)
            assert observed_status and observed_status["connected"], "test bridge not connected"
            result = subprocess.run(["node", str(actor)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=920 if args.persistence else 320)
            print(result.stdout, end="", flush=True)
            if result.stderr:
                print(result.stderr, flush=True)
            time.sleep(10)
    finally:
        if bridge and bridge.poll() is None:
            bridge.terminate()
            bridge.wait(timeout=15)
        server.close()
    client = json.loads(paths[".client.json"].read_text())
    with paths[".raw.ndjson"].open() as raw:
        capture_header = json.loads(raw.readline())
    registry_records = []
    for path in sorted(state_dir.glob("*/assembly-instances/*.json")):
        record = json.loads(path.read_text())
        registry_records.append({"path": str(path), "sha256": digest(path),
                                 "schema": record["schema"], "instance_id": record["instance_id"],
                                 "state": record["state"], "revision": record["revision"], "attempts": record["attempts"]})
    summary = {"schema_version": "dustroute.custom-assembly-comparison.v1", "status": client["status"],
               "registry_records": registry_records,
               "initial_server_status": observed_status, "actor_sha256": source_hashes[str(actor)],
               "loaded_sources_sha256": source_hashes,
               "continuous_construction_capture": args.capture_construction,
               "actual_capture_mode": capture_header["capture_mode"],
               "cleanup": client["cleanup"], "error": client.get("error"),
               "evidence_limit": "public MCP staged construction with server-confirmed readback receipts; actor probes remain client observations; no hidden readiness or world-lock claim",
               "artifacts": {s: str(p) for s,p in paths.items()},
               "sha256": {s: digest(p) for s,p in paths.items() if p.exists()}}
    save(paths[".summary.json"], summary)
    print(json.dumps({"status":client["status"], "summary":str(paths[".summary.json"])}), flush=True)
    if client["status"] != "passed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
