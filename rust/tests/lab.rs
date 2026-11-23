//! Tests for the lab itself. Mirrors python/tests/test_lab.py, pinning the
//! experiment's claims so a refactor cannot quietly change the story.

use proofgap::checker::{Oracle, Status, boundary_witnesses, overflow_obligations, prove};
use proofgap::compiler::{Bugs, to_source};
use proofgap::generated::{
    baseline, fee_helper_drift, fee_helper_drift_high_value, fee_not_debited,
};
use proofgap::interp::broken_promises;
use proofgap::spec;
use proofgap::tiers::{Outcome, TIERS, run_tier};
use proofgap::variants;

/// variant -> tiers expected to catch it. Identical to the Python table except
/// "fee helper drift" / property: proptest samples integer ranges uniformly,
/// while Hypothesis favours small and repeated values, which hit that bug.
const EXPECTED: &[(&str, &[&str])] = &[
    ("baseline (compiled)", &[]),
    ("fee not debited", &["unit", "property", "proof"]),
    ("fee helper drift", &["proof"]),
    ("fee helper drift, ≥ £1m only", &["proof"]),
    ("fee paid to payee", &["unit"]),
    (
        "compiler: associativity",
        &["unit", "property", "conformance"],
    ),
    ("compiler: ceil template", &["unit", "conformance"]),
    ("legacy service", &[]),
    ("legacy: strict check", &["conformance"]),
];

#[test]
fn spec_promises_are_proved() {
    let results = prove(&spec::spec());
    assert_eq!(results.len(), 5);
    assert!(
        results.iter().all(|r| r.status == Status::Proved),
        "{results:?}"
    );
}

#[test]
fn correct_compiler_matches_proved_semantics_on_boundaries() {
    let s = spec::spec();
    let oracle = Oracle::new(&s);
    for w in boundary_witnesses(&s, 2) {
        let (a, b, c) = (w[0].1, w[1].1, w[2].1);
        assert_eq!(baseline(a, b, c), oracle.eval(a, b, c), "{w:?}");
    }
}

#[test]
fn assoc_bug_changes_emitted_source() {
    let good = to_source(&spec::spec(), "f", Bugs::default());
    let bad = to_source(
        &spec::spec(),
        "f",
        Bugs {
            assoc_bug: true,
            ceil_bug: false,
        },
    );
    assert!(good.contains("src - (amount + fee_due)"));
    assert!(bad.contains("src - amount + fee_due"));
}

#[test]
fn proof_counterexamples_reproduce_on_the_executable() {
    let cases: [(
        fn() -> proofgap::dsl::Program,
        fn(i64, i64, i64) -> (i64, i64, i64),
    ); 3] = [
        (spec::spec_fee_helper_drift, fee_helper_drift),
        (
            spec::spec_fee_helper_drift_high_value,
            fee_helper_drift_high_value,
        ),
        (spec::spec_fee_not_debited, fee_not_debited),
    ];
    for (make, f) in cases {
        let refuted: Vec<_> = prove(&make())
            .into_iter()
            .filter_map(|r| match r.status {
                Status::Refuted(c) => Some((r.promise, c)),
                _ => None,
            })
            .collect();
        assert!(!refuted.is_empty());
        for (promise, cex) in refuted {
            let inputs: Vec<(&str, i64)> = cex.iter().map(|(n, v)| (n.as_str(), *v)).collect();
            let broken = broken_promises(&spec::spec(), &inputs, f(cex[0].1, cex[1].1, cex[2].1));
            assert!(
                broken.contains(&promise),
                "{promise} did not reproduce: {broken:?}"
            );
        }
    }
}

#[test]
fn no_subterm_can_overflow_i64() {
    let (checked, overflowing) = overflow_obligations(&spec::spec());
    assert!(checked > 0);
    assert!(overflowing.is_empty(), "{overflowing:?}");
}

#[test]
fn detection_matrix() {
    for v in variants() {
        let caught: Vec<&str> = TIERS
            .iter()
            .copied()
            .filter(|t| run_tier(t, &v).status == Outcome::Caught)
            .collect();
        let expected = EXPECTED
            .iter()
            .find(|(n, _)| *n == v.name)
            .expect("listed")
            .1;
        assert_eq!(caught, expected, "{}", v.name);
    }
}
