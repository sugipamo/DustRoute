"""
Benchmark harness for the DustRoute baseline compiler.

Measures wall time, routing effort and physical size for each target
circuit, with an optional cProfile hot-spot report.

Usage:
    python -m scripts.benchmark                 # all targets, summary table
    python -m scripts.benchmark --profile       # + cProfile top functions
    python -m scripts.benchmark --only mux2     # subset by name
    python -m scripts.benchmark --bits 8        # ripple adder width sweep
"""

from __future__ import annotations

import argparse
import cProfile
import io
import pstats
import time
from dataclasses import dataclass

from dustroute import BaselineCompiler, BaselineCompileConfig
from dustroute.dag_circuits import (
    decoder1to2_dag,
    full_adder_dag,
    half_subtractor_dag,
    mux2_dag,
    ripple_adder_2bit_dag,
)
from dustroute.logic_dag import DAGBuilder, LogicDAG
from dustroute.model import GateKind


def ripple_carry_adder_dag(bits: int) -> LogicDAG:
    """N-bit ripple-carry adder built from full-adder logic."""
    if bits < 1:
        raise ValueError("bits must be >= 1")
    b = DAGBuilder()
    outputs: list[tuple[str, int]] = []

    for i in range(bits):
        a = b.input(f"a{i}")
        bi = b.input(f"b{i}")
        if i == 0:
            outputs.append((f"s{i}", b.op(GateKind.XOR, a, bi, name=f"sum{i}")))
            prev_carry = b.op(GateKind.AND, a, bi, name=f"c{i}")
            continue
        ab = b.op(GateKind.XOR, a, bi, name=f"ab{i}_xor")
        sum_i = b.op(GateKind.XOR, ab, prev_carry, name=f"sum{i}")
        c_ab = b.op(GateKind.AND, a, bi, name=f"c{i}_ab")
        c_in = b.op(GateKind.AND, prev_carry, ab, name=f"c{i}_cin")
        prev_carry = b.op(GateKind.OR, c_ab, c_in, name=f"c{i}")
        outputs.append((f"s{i}", sum_i))

    outputs.append(("carry", prev_carry))
    return b.finish(tuple(outputs))


@dataclass
class BenchResult:
    name: str
    ok: bool
    seconds: float
    nets: int
    wires: int | None
    volume: int | None
    error: str | None = None


def _run(name: str, dag: LogicDAG, config: BaselineCompileConfig) -> BenchResult:
    start = time.perf_counter()
    try:
        result = BaselineCompiler(config).compile(dag)
    except Exception as exc:  # noqa: BLE001 - benchmark reports failures as data
        return BenchResult(name, False, time.perf_counter() - start, len(dag.nodes), None, None, str(exc))
    elapsed = time.perf_counter() - start
    wires = sum(net.wire_count for net in result.routing.nets.values())
    lo, hi = result.world.bounds()
    volume = (hi.x - lo.x + 1) * (hi.y - lo.y + 1) * (hi.z - lo.z + 1)
    return BenchResult(name, True, elapsed, len(result.logical.nets), wires, volume)


TARGETS: dict[str, callable] = {
    "mux2": mux2_dag,
    "decoder1to2": decoder1to2_dag,
    "half_subtractor": half_subtractor_dag,
    "full_adder": full_adder_dag,
    "ripple_adder_2bit": ripple_adder_2bit_dag,
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", nargs="*", help="target names to run")
    parser.add_argument("--bits", type=int, default=4, help="max ripple adder width")
    parser.add_argument("--spacing-x", type=int, default=12)
    parser.add_argument("--lane-gap", type=int, default=8)
    parser.add_argument("--no-ripup", action="store_true")
    parser.add_argument("--profile", action="store_true", help="cProfile each target")
    args = parser.parse_args()

    config = BaselineCompileConfig(
        spacing_x=args.spacing_x,
        lane_gap=args.lane_gap,
        allow_ripup=not args.no_ripup,
    )

    names = args.only or list(TARGETS) + [f"ripple_adder_{n}bit" for n in range(3, args.bits + 1)]
    results: list[BenchResult] = []

    for name in names:
        if name.startswith("ripple_adder_") and name.endswith("bit"):
            bits = int(name.removeprefix("ripple_adder_").removesuffix("bit"))
            dag_fn = lambda n=bits: ripple_carry_adder_dag(n)
        else:
            dag_fn = TARGETS[name]

        print(f"[run] {name} ...", flush=True)
        if args.profile:
            profiler = cProfile.Profile()
            profiler.enable()
        res = _run(name, dag_fn(), config)
        if args.profile:
            profiler.disable()
            stream = io.StringIO()
            pstats.Stats(profiler, stream=stream).sort_stats("cumulative").print_stats(15)
            print(stream.getvalue())
        status = "OK " if res.ok else "FAIL"
        size = f"wires={res.wires} vol={res.volume}" if res.ok else ""
        err = "" if res.error is None else f" :: {res.error}"
        print(
            f"{status} {name:>22}  {res.seconds:8.3f}s  nets={res.nets:<5} {size}{err}",
            flush=True,
        )
        results.append(res)

    print("\n=== summary ===")
    passed = [r for r in results if r.ok]
    failed = [r for r in results if not r.ok]
    total = sum(r.seconds for r in results)
    print(f"{len(passed)} passed, {len(failed)} failed, total {total:.3f}s")
    for r in failed:
        print(f"  FAIL {r.name}: {r.error}")


if __name__ == "__main__":
    main()
