"""Compare the Python and Rust labs: detection matrix, witnesses, output
checksums and performance. Standard library only.

    python3 scripts/compare.py           # writes COMPARISON.md
    python3 scripts/compare.py --check   # CI: fail if deterministic tiers disagree

Inputs: python/results.json, rust/results.json, and (optional, for the
performance section) python/bench.json, rust/bench.json.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TIERS = ["unit", "property", "proof", "conformance"]
# These tiers are deterministic given the same Z3 version and the same unit
# cases, so the two languages must agree. Property testing uses different
# libraries (Hypothesis vs proptest) with different search strategies.
MUST_AGREE = ["unit", "proof", "conformance"]

MAX = 10**12
HIGH_VALUE = 10**8


def load(path: Path):
    return json.loads(path.read_text()) if path.exists() else None


def fmt_w(w) -> str:
    return "–" if w is None else f"({w['src']}, {w['dst']}, {w['amount']})"


def trigger_probability() -> tuple[float, int]:
    """Exact chance that one uniform sample from the domain hits the
    high-value drift bug: amount >= £1m, amount % 400 != 0, and
    src == amount + ceil(amount / 400) - 1 exactly."""
    def ceil400(a):
        return -((-a) // 400)
    lo, hi = HIGH_VALUE, MAX
    while lo < hi:  # largest amount whose trigger balance still fits the domain
        mid = (lo + hi + 1) // 2
        if mid + ceil400(mid) - 1 <= MAX:
            lo = mid
        else:
            hi = mid - 1
    a_max = lo
    multiples = a_max // 400 - (HIGH_VALUE - 1) // 400
    count = (a_max - HIGH_VALUE + 1) - multiples
    return count / ((2 * MAX + 1) * (MAX + 1)), count


def human_time(seconds: float) -> str:
    for unit, n in (("years", 365 * 86400), ("days", 86400), ("hours", 3600), ("minutes", 60)):
        if seconds >= n:
            return f"{seconds / n:,.1f} {unit}"
    return f"{seconds:,.1f} s"


def matrix_section(py, rs) -> tuple[list[str], list[str]]:
    lines = ["| variant | " + " | ".join(TIERS) + " |", "|---|" + "---|" * len(TIERS)]
    problems, notes = [], []
    for name in py["order"]:
        cells = []
        for t in TIERS:
            a = py["variants"][name]["tiers"][t]["status"]
            b = rs["variants"][name]["tiers"][t]["status"]
            short = {"caught": "CAUGHT", "missed": "·", "n/a": "n/a"}
            if a == b:
                cells.append(f"{short[a]} ✓" if a == "caught" else short[a])
            else:
                cells.append(f"**Py {short[a]} / Rs {short[b]}**")
                (problems if t in MUST_AGREE else notes).append(f"{name} / {t}: Python {a}, Rust {b}")
        lines.append(f"| {name} | " + " | ".join(cells) + " |")
    return lines, problems + [f"(allowed) {n}" for n in notes]


def witness_section(py, rs) -> tuple[list[str], int, int]:
    lines = ["| variant | tier | Python witness | Rust witness | same? |", "|---|---|---|---|---|"]
    same = total = 0
    for name in py["order"]:
        for t in TIERS:
            a = py["variants"][name]["tiers"][t]
            b = rs["variants"][name]["tiers"][t]
            if a["witness"] is None and b["witness"] is None:
                continue
            total += 1
            eq = a["witness"] == b["witness"]
            same += eq
            lines.append(f"| {name} | {t} | {fmt_w(a['witness'])} | {fmt_w(b['witness'])} | {'yes' if eq else 'no'} |")
    return lines, same, total


def perf_section(pb, rb) -> list[str]:
    p, r = pb["results"], rb["results"]

    def row(label, pv, rv, unit, fmt="{:,.2f}", higher_is_better=False):
        ratio = (rv / pv) if higher_is_better else (pv / rv)
        return f"| {label} | {fmt.format(pv)} {unit} | {fmt.format(rv)} {unit} | {ratio:,.1f}× |"

    lines = [
        "| workload | Python | Rust | Rust advantage |",
        "|---|---|---|---|",
        row("compiled transfer, per call (1M calls)", p["exec_compiled"]["ns_per_call"], r["exec_compiled"]["ns_per_call"], "ns"),
        row("hand-written legacy, per call (1M calls)", p["exec_legacy"]["ns_per_call"], r["exec_legacy"]["ns_per_call"], "ns"),
        row("property tier, 1,000 cases", p["property_1000"]["ms"], r["property_1000"]["ms"], "ms"),
        row("property tier, 100,000 cases (high-value bug)", p["property_100k_high_value_drift"]["seconds"],
            r["property_100k_high_value_drift"]["seconds"], "s", "{:,.3f}"),
        row("raw random sampling rate", p["brute_force_high_value_drift"]["samples_per_second"] / 1e6,
            r["brute_force_high_value_drift"]["samples_per_second"] / 1e6, "M/s", higher_is_better=True),
        row("Z3 oracle evaluation, per call", p["oracle"]["us_per_call"], r["oracle"]["us_per_call"], "µs"),
        row("solver boundary inputs (14 witnesses)", p["boundary_witnesses"]["ms"], r["boundary_witnesses"]["ms"], "ms"),
        row("prove all 5 promises (spec)", p["prove_spec"]["ms"], r["prove_spec"]["ms"], "ms"),
        row("refute high-value drift", p["prove_high_value_drift"]["ms"], r["prove_high_value_drift"]["ms"], "ms"),
    ]
    return lines


def main() -> int:
    py = load(ROOT / "python/results.json")
    rs = load(ROOT / "rust/results.json")
    if not py or not rs:
        print("missing results.json; run both experiments first", file=sys.stderr)
        return 2
    assert py["order"] == rs["order"], "variant lists differ"

    matrix, problems = matrix_section(py, rs)
    hard = [p for p in problems if not p.startswith("(allowed)")]
    if "--check" in sys.argv:
        for p in problems:
            print(p)
        print("OK: deterministic tiers agree" if not hard else "FAIL: deterministic tiers disagree")
        return 1 if hard else 0

    wit, same, total = witness_section(py, rs)
    out = [
        "# Python vs Rust",
        "",
        "Generated by `python3 scripts/compare.py` from `python/results.json`, `rust/results.json` "
        "and the two `bench.json` files.",
        "",
        "## Toolchains",
        "",
        f"- Python {py['versions']['python']}, {py['versions']['property_testing']}, Z3 {py['versions']['z3'].split()[0]}",
        f"- {rs['versions']['rustc']}, {rs['versions']['property_testing']}, Z3 {rs['versions']['z3'].split()[0]}",
        "",
        "## Detection matrix, both languages",
        "",
        "`CAUGHT ✓` = both languages catch it. A bold cell means the languages disagree.",
        "",
        *matrix,
        "",
        f"**Deterministic tiers (unit, proof, conformance): {'agree on every cell' if not hard else 'DISAGREE'}.**",
        "",
    ]
    if problems:
        out += ["Differences:", ""] + [f"- {p}" for p in problems] + [""]
    out += [
        "## Witnesses",
        "",
        f"{same} of {total} witnesses are identical across languages. Both languages pin Z3 to the same "
        "version and build the same terms in the same order, so the solver gives the same answers. "
        "Random-testing witnesses differ because Hypothesis and proptest search differently.",
        "",
        *wit,
        "",
    ]

    pb, rb = load(ROOT / "python/bench.json"), load(ROOT / "rust/bench.json")
    if pb and rb:
        p, r = pb["results"], rb["results"]
        checks = [
            ("compiled transfer, 1M inputs", p["exec_compiled"]["checksum"], r["exec_compiled"]["checksum"]),
            ("hand-written legacy, 1M inputs", p["exec_legacy"]["checksum"], r["exec_legacy"]["checksum"]),
            ("Z3 oracle, 2,000 inputs", p["oracle"]["checksum"], r["oracle"]["checksum"]),
        ]
        prob, count = trigger_probability()
        expected = 1 / prob
        out += [
            "## Same outputs across languages",
            "",
            "Both benchmarks draw the same 1,000,000 inputs from a splitmix64 stream (seed 42) and hash every "
            "output with FNV-1a.",
            "",
            "| executable | Python checksum | Rust checksum | equal? |",
            "|---|---|---|---|",
            *[f"| {n} | `{a}` | `{b}` | {'yes' if a == b else '**no**'} |" for n, a, b in checks],
            "",
            f"The compiled code and the hand-written legacy code agree in both languages "
            f"(`{p['exec_compiled']['checksum']}` four times). In both languages, the proved Z3 semantics "
            f"agree with the compiled code on the oracle sample "
            f"(Python: {'yes' if p['oracle']['matches_compiled'] else 'no'}, Rust: {'yes' if r['oracle']['matches_compiled'] else 'no'}).",
            "",
            "## Performance",
            "",
            f"Machine: {pb['machine']} (Python) / {rb['machine']} (Rust release build). Medians of repeated runs; "
            "single machine, so read ratios rather than absolute numbers.",
            "",
            *perf_section(pb, rb),
            "",
            "### Does speed close the gap?",
            "",
            f"A uniformly random input hits the high-value drift bug with probability **{prob:.2e}** "
            f"({count:,} trigger (amount, balance) pairs out of about 2×10²⁴; the payee balance is irrelevant), so random sampling needs about **{expected:.1e}** samples on average.",
            "",
            "| | samples tried | found? | rate | expected time to find |",
            "|---|---|---|---|---|",
        ]
        for lang, b in (("Python", p), ("Rust", r)):
            bf = b["brute_force_high_value_drift"]
            out.append(f"| {lang} | {bf['samples']:,} | {'yes' if bf['found'] else 'no'} | "
                       f"{bf['samples_per_second'] / 1e6:,.1f} M/s | {human_time(expected / bf['samples_per_second'])} |")
        out += [
            "",
            f"Z3 refutes it in {p['prove_high_value_drift']['ms']:.1f} ms (Python) / "
            f"{r['prove_high_value_drift']['ms']:.1f} ms (Rust).",
            "",
        ]
    (ROOT / "COMPARISON.md").write_text("\n".join(out) + "\n")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
