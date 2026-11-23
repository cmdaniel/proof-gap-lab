"""Tests for the lab itself. They pin the experiment's claims so a refactor
cannot quietly change the story the README tells."""

import pytest

from proofgap import spec
from proofgap.checker import Oracle, boundary_witnesses, prove
from proofgap.compiler import compile_program
from proofgap.experiment import variants
from proofgap.interp import broken_promises
from proofgap.tiers import TIERS

# variant -> set of tiers expected to catch it
EXPECTED = {
    "baseline (compiled)": set(),
    "fee not debited": {"unit", "property", "proof"},
    "fee helper drift": {"property", "proof"},
    "fee helper drift, ≥ £1m only": {"proof"},
    "fee paid to payee": {"unit"},
    "compiler: associativity": {"unit", "property", "conformance"},
    "compiler: ceil template": {"unit", "conformance"},
    "legacy service": set(),
    "legacy: strict check": {"conformance"},
}


def test_spec_promises_are_proved():
    assert {r.promise: r.status for r in prove(spec.SPEC)} == {p: "proved" for p in spec.PROMISES}


def test_correct_compiler_matches_proved_semantics_on_boundaries():
    fn, _ = compile_program(spec.SPEC)
    oracle = Oracle(spec.SPEC)
    for w in boundary_witnesses(spec.SPEC):
        assert fn(**w) == oracle(**w), w


def test_assoc_bug_changes_emitted_source():
    good = compile_program(spec.SPEC)[1]
    bad = compile_program(spec.SPEC, assoc_bug=True)[1]
    assert "src - (amount + fee_due)" in good
    assert "src - amount + fee_due" in bad


@pytest.mark.parametrize("make", [spec.spec_fee_helper_drift, spec.spec_fee_helper_drift_high_value,
                                  spec.spec_fee_not_debited])
def test_proof_counterexamples_reproduce_on_the_executable(make):
    """A refutation is only useful if it is real: replay it on compiled code."""
    prog = make()
    fn, _ = compile_program(prog)
    refuted = [r for r in prove(prog) if r.status == "refuted"]
    assert refuted
    for r in refuted:
        assert r.promise in broken_promises(spec.SPEC, r.counterexample, fn(**r.counterexample))


@pytest.mark.parametrize("v", variants(), ids=lambda v: v.name)
def test_detection_matrix(v):
    caught = {t for t, run in TIERS.items() if run(v).status == "caught"}
    assert caught == EXPECTED[v.name]
