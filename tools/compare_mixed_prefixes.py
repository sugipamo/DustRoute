#!/usr/bin/env python3
"""Compare before-next-input client samples with settled model prefixes.

Only prefixes whose modeled idle time precedes the next actual server input
are compared. This does not claim an exact client-sample server tick.
"""
from electrical_fixture_adapter import replay_fixture

import argparse
import json
from pathlib import Path

from observation_fixture import ROOT
from observation_records import digest, key, properties, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    args = parser.parse_args()
    trial = json.loads(args.prefix.with_suffix(".input.json").read_text())
    client = json.loads(args.prefix.with_suffix(".client.json").read_text())
    observations = []
    for index in range(1, len(trial["inputs"])):
        path = args.prefix.with_suffix(f".prefix-{index}.input.json")
        save(path, {**trial, "inputs": trial["inputs"][:index]})
        run = replay_fixture(path, check=True)
        model = json.loads(run.stdout)
        if model["final_time"]["game_tick"] >= trial["inputs"][index]["tick"]:
            continue
        sample, = [s for s in client["samples"] if s["label"] == f"before_{index}"]
        modeled = {key(b["position"]): (b["name"],b["properties"]) for b in model["blocks"] if b["name"] and b["name"] != "minecraft:air"}
        blocks = [{"position":b["position"],"name":b["name"],"properties":properties(b["properties"])} for b in sample["blocks"]]
        differences = [b for b in blocks if (b["name"],b["properties"]) != modeled.get(key(b["position"]),("minecraft:air",{}))]
        observations.append({"input_count":index, "sample_label":sample["label"], "client_tick":sample["client_tick"], "blocks":blocks,
                             "classification":"matched_sample" if not differences else "model_state_mismatch", "differences":differences})
    result = {"source_client_sha256":digest(args.prefix.with_suffix(".client.json")),
              "evidence_limit":"client samples before the next interaction; actual applied input intervals replayed, no exact sample tick claim",
              "observations":observations}
    save(args.prefix.with_suffix(".prefixes.json"),result)
    print(json.dumps({"samples":len(observations),"mismatches":sum(o["classification"] != "matched_sample" for o in observations)}))
    if any(o["classification"] != "matched_sample" for o in observations):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
