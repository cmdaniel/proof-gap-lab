"""Performance benchmarks. The Rust lab runs the same workloads on the same
deterministic input stream (splitmix64, seed 42), so checksums must match.

    uv run python -m proofgap.bench          # writes bench.json
"""

from __future__ import annotations

import json
import platform
import statistics
import time
from pathlib import Path

from . import spec
from .checker import Oracle, boundary_witnesses, prove
from .compiler import compile_program
from .legacy import LedgerService, as_function
from .tiers import Variant, property_tier

MASK = (1 << 64) - 1
FNV_PRIME = 0x100000001B3
FNV_OFFSET = 0xCBF29CE484222325
MAX = spec.MAX_BALANCE


class SplitMix64:
    def __init__(self, seed: int):
        self.state = seed & MASK

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def transfer_input(self) -> tuple[int, int, int]:
        src = self.next() % (MAX + 1)
        dst = self.next() % (MAX + 1)
        amount = self.next() % (2 * MAX + 1) - MAX
        return src, dst, amount


def inputs(n: int) -> list[tuple[int, int, int]]:
    rng = SplitMix64(42)
    return [rng.transfer_input() for _ in range(n)]


def checksum(outputs) -> int:
    h = FNV_OFFSET
    for triple in outputs:
        for x in triple:
            h = ((h ^ (x & MASK)) * FNV_PRIME) & MASK
    return h


def timed(fn, repeat: int):
    times, out = [], None
    for _ in range(repeat):
        t = time.perf_counter()
        out = fn()
        times.append(time.perf_counter() - t)
    return statistics.median(times), out


def bench_exec(fn, xs):
    def run():
        return checksum(fn(s, d, a) for s, d, a in xs)
    secs, h = timed(run, 3)
    return {"calls": len(xs), "seconds": secs, "ns_per_call": secs / len(xs) * 1e9, "checksum": f"{h:016x}"}


def brute_force(fn, n: int):
    """Plain random sampling for the high-value drift bug, no framework."""
    rng = SplitMix64(7)
    t = time.perf_counter()
    found = None
    for _ in range(n):
        s, d, a = rng.transfer_input()
        if fn(s, d, a)[0] < 0:
            found = {"src": s, "dst": d, "amount": a}
            break
    secs = time.perf_counter() - t
    return {"samples": n, "seconds": secs, "samples_per_second": n / secs, "found": found}


def main() -> None:
    xs = inputs(1_000_000)
    baseline, _ = compile_program(spec.SPEC)
    legacy = as_function(LedgerService())
    hv_prog = spec.spec_fee_helper_drift_high_value()
    hv, _ = compile_program(hv_prog)

    results = {}
    print("exec ...", flush=True)
    results["exec_compiled"] = bench_exec(baseline, xs)
    results["exec_legacy"] = bench_exec(legacy, xs)

    print("oracle ...", flush=True)
    oracle = Oracle(spec.SPEC)
    few = xs[:2_000]
    secs, h = timed(lambda: checksum(oracle(src=s, dst=d, amount=a) for s, d, a in few), 3)
    results["oracle"] = {"calls": len(few), "seconds": secs, "us_per_call": secs / len(few) * 1e6,
                         "checksum": f"{h:016x}",
                         "matches_compiled": h == checksum(baseline(s, d, a) for s, d, a in few)}

    print("proof ...", flush=True)
    secs, r = timed(lambda: prove(spec.SPEC), 20)
    results["prove_spec"] = {"promises": len(r), "ms": secs * 1e3,
                             "all_proved": all(x.status == "proved" for x in r)}
    secs, r = timed(lambda: prove(hv_prog), 20)
    results["prove_high_value_drift"] = {"ms": secs * 1e3,
                                         "refuted": [x.promise for x in r if x.status == "refuted"]}
    secs, w = timed(lambda: boundary_witnesses(spec.SPEC), 5)
    results["boundary_witnesses"] = {"ms": secs * 1e3, "count": len(w)}

    print("property tier ...", flush=True)
    base_v = Variant("baseline", "-", "", baseline, spec.SPEC, spec.SPEC)
    secs, _ = timed(lambda: property_tier(base_v, 1_000), 3)
    results["property_1000"] = {"ms": secs * 1e3}
    hv_v = Variant("hv", "source", "", hv, hv_prog, hv_prog)
    secs, r = timed(lambda: property_tier(hv_v, 100_000), 1)
    results["property_100k_high_value_drift"] = {"seconds": secs, "status": r.status}

    print("brute force ...", flush=True)
    results["brute_force_high_value_drift"] = brute_force(hv, 2_000_000)

    out = {"language": "python", "python": platform.python_version(),
           "machine": f"{platform.system()} {platform.machine()}", "results": results}
    Path("bench.json").write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
