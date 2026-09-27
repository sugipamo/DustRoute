#!/usr/bin/env python3
"""Create a distinct, explicitly revised downward Assembly for the live trial."""
import argparse
import copy
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    fixture = json.loads(args.source.read_text())
    request = fixture["request"]
    replacements = {
        request["candidate_parent"]: "runtime-test.down-parent.v2",
        request["next_child"]: "runtime-test.down-mechanism.v2",
        request["candidate_state"]["id"]: "runtime-test.down-state.v2",
    }

    def rename(value):
        if isinstance(value, dict):
            return {k: rename(v) for k, v in value.items()}
        if isinstance(value, list):
            return [rename(v) for v in value]
        return replacements.get(value, value) if isinstance(value, str) else value

    request = rename(request)
    mechanism, = [r for r in request["revisions"] if r["id"] == request["next_child"]]
    for row in mechanism["blocks"]:
        pos = row["position"]
        if pos["x"] <= 0:
            if pos == {"x": 0, "y": 2, "z": 0}:
                pos["y"] = 0  # Payload on the new downward front before lifting the circuit.
            pos["y"] += 3
            if row["block"]["kind"] == "Piston":
                row["block"]["facing"] = "Down"
        elif pos == {"x": 16, "y": 3, "z": 0}:
            pos["y"] = 5
        elif pos == {"x": 16, "y": 4, "z": 0}:
            row["block"]["facing"] = "Up"
    for port in mechanism["ports"]:
        port["position"]["y"] += 3
    for lever in request["behavior_context"]["input_levers"]:
        lever["y"] += 3
    request["candidate_state"]["assembly"]["blocks"] = copy.deepcopy(mechanism["blocks"])
    request["title"] = "Explicit downward mixed Assembly revision"
    request["description"] = "Move the controlled circuit upward, face its body downward, and swap the constant vertical body upward. Retain the old source definitions."
    fixture["request"] = request
    fixture["probe_body"] = {"x": 0, "y": 4, "z": 0}
    with args.output.open("x") as output:
        json.dump(fixture, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
