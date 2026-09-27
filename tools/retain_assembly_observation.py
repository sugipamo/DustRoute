#!/usr/bin/env python3
"""Retain public MCP results and actual server-applied inputs from a live trial.

The artifact is evidence for this trial, never a reusable placement authority.
Bounded raw capture does not establish complete construction callback coverage.
"""
import argparse
import json
from pathlib import Path

from observe_mixed_pistons import applied_inputs, digest, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    summary = json.loads(args.prefix.with_suffix(".summary.json").read_text())
    assert summary["status"] == "passed"
    for suffix, expected in summary["sha256"].items():
        assert digest(args.prefix.with_suffix(suffix)) == expected, f"changed artifact {suffix}"
    client = json.loads(args.prefix.with_suffix(".client.json").read_text())
    assert client["status"] == "passed" and client["cleanup"]["region_empty"]
    assert client["cleanup"]["force_load_removed"]
    raw = [json.loads(line) for line in args.prefix.with_suffix(".raw.ndjson").read_text().splitlines()]
    inputs = applied_inputs(raw, {"activations": [
        {"position": i["position"], "requested_level": i["powered"]} for i in client["inputs"]
    ]})
    calls = []
    for call in client["calls"]:
        response = call["response"]
        calls.append({"name": call["name"], **{k: response[k] for k in (
            "ok", "operation_id", "instance_id", "status", "verified_steps", "total_steps", "error", "undo"
        ) if k in response}, **({"action": call["args"]["action"],
            "observation_status": response.get("observation", {}).get("status"),
            "revalidation_status": response.get("observation", {}).get("revalidation", {}).get("status"),
            "instance_state": response.get("instance", {}).get("state")}
            if call["name"] == "manage_assembly" else {})})
    verified = [c for c in calls if c.get("status") == "verified"]
    assert len(verified) == 2 and all(c["verified_steps"] == c["total_steps"] > 0 for c in verified)
    if client.get("persistence_checks"):
        assert all(client["persistence_checks"].values())
        assert client["restart"]["after_placement"] and client["restart"]["after_removal"]
        assert len(client["process_restarts"]) == 3
        assert all(r["before_pid"] != r["after_pid"] for r in client["process_restarts"])
        records = summary["registry_records"]
        assert len(records) == 1 and records[0]["state"] == "removed"
        for record in records:
            assert digest(Path(record["path"])) == record["sha256"]
            assert len(record["attempts"]) == 2
            assert all(a["verified_steps"] == a["total_steps"] > 0 and a["error"] is None for a in record["attempts"])
        states = {c.get("observation_status") for c in calls}
        assert {"matches", "changed", "observation_incomplete"} <= states
    save(args.output, {
        "schema_version": "dustroute.custom-assembly-live-evidence.v1",
        "run_id": args.prefix.name, "status": "passed", "origin": client["origin"],
        "known_region": client["known_region"], "server": summary["initial_server_status"],
        "calls": calls, "applied_inputs": inputs, "body_readbacks": client["inputs"],
        "restart": client.get("restart"), "rotation": client.get("rotation", "r90"),
        "process_restarts": client.get("process_restarts", []),
        "persistence_checks": client.get("persistence_checks"),
        "registry_records": summary.get("registry_records", []),
        "cleanup": client["cleanup"], "artifacts_sha256": summary["sha256"],
        "actor_sha256": summary["actor_sha256"], "capture_footer": raw[-1],
        "evidence_limit": "Public MCP per-stage whole-region verification and client body readback after applied inputs. Client sample ticks are not server ticks; bounded server capture omits earlier construction and is not complete tick-internal evidence.",
    })
    print(json.dumps({"inputs": len(inputs), "verified_operations": len(verified), "output": str(args.output)}))


if __name__ == "__main__":
    main()
