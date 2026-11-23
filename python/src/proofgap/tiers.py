"""Four reliability tiers, each run against one variant of the system.

  unit         example-based tests written from business requirements
  property     random inputs (Hypothesis) checked against the core promises
  proof        Z3 proves the promises for ALL inputs of the *source*
  conformance  the *executable* compared with the proved semantics, on
               solver-generated boundary inputs plus random inputs
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from .checker import Oracle, boundary_witnesses, prove
from .dsl import Program
from .interp import broken_promises
from .spec import SPEC

Artifact = Callable[..., tuple[int, int, int]]


@dataclass
class Variant:
    name: str
    layer: str            # where the bug lives: "-", "source", "compiler", "legacy"
    story: str
    artifact: Artifact    # what actually executes
    source: Program | None  # what the proof sees (None: not written in the DSL)
    reference: Program    # the proved spec the artifact is meant to implement
    generated: str | None = None  # compiler output, for display


@dataclass
class TierResult:
    status: str  # "caught" | "missed" | "n/a"
    detail: str = ""
    witness: dict[str, int] | None = None  # the input that exposed the bug
    seconds: float = 0.0


# --- Tier 1: unit tests ----------------------------------------------------
# Written by a developer from the product requirements, before the spec
# existed. Typical, sensible, and finite.
UNIT_CASES = [
    # (src, dst, amount) -> (new_src, new_dst, fee)
    ((50_000, 0, 10_000), (39_975, 10_000, 25)),   # £100 transfer, 25p fee
    ((5_000, 1_000, 1_000), (3_997, 2_000, 3)),    # 2.5p fee rounds up to 3p
    ((100, 0, 200), (100, 0, 0)),                  # insufficient funds
    ((100, 0, 0), (100, 0, 0)),                    # zero amount
    ((100, 0, -50), (100, 0, 0)),                  # negative amount
]


def unit_tier(v: Variant) -> TierResult:
    failures = []
    for (s, d, a), expected in UNIT_CASES:
        got = v.artifact(src=s, dst=d, amount=a)
        if got != expected:
            failures.append((f"transfer({s}, {d}, {a}) = {got}, expected {expected}",
                             {"src": s, "dst": d, "amount": a}))
    if failures:
        msg, w = failures[0]
        return TierResult("caught", f"{len(failures)}/{len(UNIT_CASES)} failed; e.g. {msg}", w)
    return TierResult("missed", f"{len(UNIT_CASES)}/{len(UNIT_CASES)} passed")


# --- Tier 2 & 4 shared input strategy ---------------------------------------
def _inputs(prog: Program):
    return st.fixed_dictionaries({n: st.integers(lo, hi) for n, (lo, hi) in prog.inputs.items()})


def _run_hypothesis(prog: Program, check: Callable[[dict], str | None], examples: int):
    """Run `check` on generated inputs; return (minimal failing input, message) or None."""
    found: list = []

    @settings(max_examples=examples, derandomize=True, database=None, deadline=None,
              suppress_health_check=list(HealthCheck))
    @given(_inputs(prog))
    def run(inputs):
        msg = check(inputs)
        if msg:
            found.append((inputs, msg))
            raise AssertionError(msg)

    try:
        run()
    except AssertionError:
        return found[-1]  # Hypothesis replays the shrunk example last
    return None


# --- Tier 2: property-based tests -------------------------------------------
def property_tier(v: Variant, examples: int = 1_000) -> TierResult:
    def check(inputs):
        broken = broken_promises(SPEC, inputs, v.artifact(**inputs))
        return f"broke {', '.join(broken)}" if broken else None

    hit = _run_hypothesis(SPEC, check, examples)
    if hit:
        inputs, msg = hit
        return TierResult("caught", f"{msg} at {_fmt(inputs)}", dict(inputs))
    return TierResult("missed", f"{examples} random inputs, all promises held")


# --- Tier 3: proof -----------------------------------------------------------
def proof_tier(v: Variant) -> TierResult:
    if v.source is None:
        return TierResult("n/a", "not written in the proof language; nothing to prove")
    results = prove(v.source)
    refuted = [r for r in results if r.status == "refuted"]
    unknown = [r for r in results if r.status == "unknown"]
    if refuted:
        r = refuted[0]
        return TierResult("caught", f"{r.promise} refuted, counterexample {_fmt(r.counterexample)}",
                          r.counterexample)
    if unknown:
        return TierResult("missed", f"solver gave up on {', '.join(r.promise for r in unknown)}")
    return TierResult("missed", f"all {len(results)} promises proved for every input in the domain")


# --- Tier 4: conformance (differential) testing ------------------------------
def conformance_tier(v: Variant, examples: int = 1_000) -> TierResult:
    oracle = Oracle(v.reference)

    def check(inputs):
        want, got = oracle(**inputs), v.artifact(**inputs)
        return f"executable {got} != proved semantics {want}" if want != got else None

    witnesses = boundary_witnesses(v.reference)
    for w in witnesses:
        msg = check(w)
        if msg:
            return TierResult("caught", f"solver-chosen boundary input {_fmt(w)}: {msg}", w)
    hit = _run_hypothesis(v.reference, check, examples)
    if hit:
        inputs, msg = hit
        return TierResult("caught", f"random input {_fmt(inputs)}: {msg}", dict(inputs))
    return TierResult("missed", f"{len(witnesses)} boundary + {examples} random inputs agreed")


TIERS = {
    "unit": unit_tier,
    "property": property_tier,
    "proof": proof_tier,
    "conformance": conformance_tier,
}


def _fmt(d: dict | None) -> str:
    return "{" + ", ".join(f"{k}={v}" for k, v in (d or {}).items()) + "}"
