#!/usr/bin/env python3
"""Retain a compact settled-state regression from a completed mixed live run.

The raw capture remains independently hashed. A partial global sequence stays
partial; this extraction cannot turn client readback into tick conformance.
"""
import argparse
import json
from pathlib import Path

from observe_mixed_pistons import digest, save, snapshot


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    summary = json.loads(args.prefix.with_suffix(".summary.json").read_text())
    assert summary["classification"] == "matched_final_state", "a failed run cannot become a passing observation"
    for suffix, expected in summary["sha256"].items():
        assert digest(args.prefix.with_suffix(suffix)) == expected, f"changed artifact {suffix}"
    trial = json.loads(args.prefix.with_suffix(".input.json").read_text())
    client = json.loads(args.prefix.with_suffix(".client.json").read_text())
    assert client["complete"] and client["cleanup"]["region_empty"] and client["cleanup"]["force_load_removed"]
    raw = [json.loads(line) for line in args.prefix.with_suffix(".raw.ndjson").read_text().splitlines()]
    sequences = [r["sequence"] for r in raw if "sequence" in r]
    prefixes_path = args.prefix.with_suffix(".prefixes.json")
    prefixes = []
    prefix_hash = None
    if prefixes_path.exists():
        captured = json.loads(prefixes_path.read_text())
        assert captured["source_client_sha256"] == digest(args.prefix.with_suffix(".client.json"))
        prefixes = captured["observations"]
        assert all(p["classification"] == "matched_sample" and not p["differences"] for p in prefixes)
        prefix_hash = digest(prefixes_path)
    save(args.output, {
        "schema_version": "dustroute.mixed-piston-settled-observation.v1",
        "run_id": args.prefix.name,
        "evidence_limit": "Applied lever writes and full settled client readback only. This is not a complete tick-internal conformance trace.",
        "minecraft_version": "1.21.11", "redstone_experiments": False,
        "provenance": {
            **{k: summary[k] for k in ("fixture", "fixture_sha256", "actor_sha256", "comparator_sha256")},
            "artifacts_sha256": summary["sha256"],
            "global_sequence_contiguous": summary["capture_sequence_contiguous"],
            "retained_record_sequence_contiguous": all(b == a + 1 for a, b in zip(sequences, sequences[1:])),
            "capture_footer": raw[-1], "cleanup": summary["cleanup"],
            "prefix_samples_sha256": prefix_hash,
        },
        "applied_inputs": summary["applied_inputs"], **trial,
        "observed_final": snapshot(client, "final"),
        "observed_prefixes": prefixes,
    })


if __name__ == "__main__":
    main()
