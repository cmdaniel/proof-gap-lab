//! Four reliability tiers, each run against one variant of the system.
//! Port of `python/src/proofgap/tiers.py`; proptest stands in for Hypothesis.
//!
//!   unit         example-based tests written from business requirements
//!   property     random inputs (proptest) checked against the core promises
//!   proof        Z3 proves the promises for ALL inputs of the *source*
//!   conformance  the *executable* compared with the proved semantics, on
//!                solver-generated boundary inputs plus random inputs

use std::time::Instant;

use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestError, TestRng, TestRunner};

use crate::checker::{Oracle, Status, boundary_witnesses, prove};
use crate::dsl::Program;
use crate::interp::broken_promises;
use crate::spec;

pub type Artifact = Box<dyn Fn(i64, i64, i64) -> (i64, i64, i64)>;

pub struct Variant {
    pub name: String,
    /// where the bug lives: "-", "source", "compiler", "legacy"
    pub layer: String,
    pub story: String,
    /// what actually executes
    pub artifact: Artifact,
    /// what the proof sees (None: not written in the DSL)
    pub source: Option<Program>,
    /// the proved spec the artifact is meant to implement
    pub reference: Program,
    /// compiler output, for display
    pub generated: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Caught,
    Missed,
    NotApplicable,
}

impl Outcome {
    pub fn key(self) -> &'static str {
        match self {
            Outcome::Caught => "caught",
            Outcome::Missed => "missed",
            Outcome::NotApplicable => "n/a",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TierResult {
    pub status: Outcome,
    pub detail: String,
    /// the input that exposed the bug, when there is one
    pub witness: Option<[i64; 3]>,
    pub seconds: f64,
}

fn result(status: Outcome, detail: String, witness: Option<[i64; 3]>) -> TierResult {
    TierResult {
        status,
        detail,
        witness,
        seconds: 0.0,
    }
}

pub fn fmt_inputs(w: [i64; 3]) -> String {
    format!("{{src={}, dst={}, amount={}}}", w[0], w[1], w[2])
}

// --- Tier 1: unit tests -----------------------------------------------------
pub const UNIT_CASES: &[((i64, i64, i64), (i64, i64, i64))] = &[
    ((50_000, 0, 10_000), (39_975, 10_000, 25)), // £100 transfer, 25p fee
    ((5_000, 1_000, 1_000), (3_997, 2_000, 3)),  // 2.5p fee rounds up to 3p
    ((100, 0, 200), (100, 0, 0)),                // insufficient funds
    ((100, 0, 0), (100, 0, 0)),                  // zero amount
    ((100, 0, -50), (100, 0, 0)),                // negative amount
];

pub fn unit_tier(v: &Variant) -> TierResult {
    let mut failures = vec![];
    for &((s, d, a), expected) in UNIT_CASES {
        let got = (v.artifact)(s, d, a);
        if got != expected {
            failures.push((
                format!("transfer({s}, {d}, {a}) = {got:?}, expected {expected:?}"),
                [s, d, a],
            ));
        }
    }
    let n = UNIT_CASES.len();
    match failures.first() {
        Some((msg, w)) => result(
            Outcome::Caught,
            format!("{}/{n} failed; e.g. {msg}", failures.len()),
            Some(*w),
        ),
        None => result(Outcome::Missed, format!("{n}/{n} passed"), None),
    }
}

// --- Shared random runner -----------------------------------------------------
/// Deterministic proptest run over the program's domain. Returns the shrunk
/// failing input and message, or None.
pub fn run_random(
    prog: &Program,
    cases: u32,
    check: impl Fn(i64, i64, i64) -> Option<String>,
) -> Option<([i64; 3], String)> {
    let r = |i: usize| prog.inputs[i].1.0..=prog.inputs[i].1.1;
    let strategy = (r(0), r(1), r(2));
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    match runner.run(&strategy, |(s, d, a)| match check(s, d, a) {
        Some(msg) => Err(TestCaseError::fail(msg)),
        None => Ok(()),
    }) {
        Ok(()) => None,
        Err(TestError::Fail(reason, (s, d, a))) => Some(([s, d, a], reason.message().to_string())),
        Err(TestError::Abort(reason)) => panic!("proptest aborted: {reason}"),
    }
}

// --- Tier 2: property-based tests ---------------------------------------------
pub fn property_tier(v: &Variant, cases: u32) -> TierResult {
    let promises = spec::spec();
    let hit = run_random(&promises, cases, |s, d, a| {
        let broken = broken_promises(
            &promises,
            &[("src", s), ("dst", d), ("amount", a)],
            (v.artifact)(s, d, a),
        );
        (!broken.is_empty()).then(|| format!("broke {}", broken.join(", ")))
    });
    match hit {
        Some((w, msg)) => result(
            Outcome::Caught,
            format!("{msg} at {}", fmt_inputs(w)),
            Some(w),
        ),
        None => result(
            Outcome::Missed,
            format!("{cases} random inputs, all promises held"),
            None,
        ),
    }
}

// --- Tier 3: proof --------------------------------------------------------------
pub fn proof_tier(v: &Variant) -> TierResult {
    let Some(source) = &v.source else {
        return result(
            Outcome::NotApplicable,
            "not written in the proof language; nothing to prove".into(),
            None,
        );
    };
    let results = prove(source);
    if let Some((r, cex)) = results.iter().find_map(|r| match &r.status {
        Status::Refuted(c) => Some((r, c)),
        _ => None,
    }) {
        let w = [cex[0].1, cex[1].1, cex[2].1];
        return result(
            Outcome::Caught,
            format!("{} refuted, counterexample {}", r.promise, fmt_inputs(w)),
            Some(w),
        );
    }
    let unknown: Vec<&str> = results
        .iter()
        .filter(|r| r.status == Status::Unknown)
        .map(|r| r.promise.as_str())
        .collect();
    if !unknown.is_empty() {
        return result(
            Outcome::Missed,
            format!("solver gave up on {}", unknown.join(", ")),
            None,
        );
    }
    result(
        Outcome::Missed,
        format!(
            "all {} promises proved for every input in the domain",
            results.len()
        ),
        None,
    )
}

// --- Tier 4: conformance (differential) testing ---------------------------------
pub fn conformance_tier(v: &Variant, cases: u32) -> TierResult {
    let oracle = Oracle::new(&v.reference);
    let check = |s, d, a| {
        let (want, got) = (oracle.eval(s, d, a), (v.artifact)(s, d, a));
        (want != got).then(|| format!("executable {got:?} != proved semantics {want:?}"))
    };
    let witnesses = boundary_witnesses(&v.reference, 2);
    for w in &witnesses {
        let w = [w[0].1, w[1].1, w[2].1];
        if let Some(msg) = check(w[0], w[1], w[2]) {
            return result(
                Outcome::Caught,
                format!("solver-chosen boundary input {}: {msg}", fmt_inputs(w)),
                Some(w),
            );
        }
    }
    match run_random(&v.reference, cases, check) {
        Some((w, msg)) => result(
            Outcome::Caught,
            format!("random input {}: {msg}", fmt_inputs(w)),
            Some(w),
        ),
        None => result(
            Outcome::Missed,
            format!(
                "{} boundary + {cases} random inputs agreed",
                witnesses.len()
            ),
            None,
        ),
    }
}

pub const TIERS: [&str; 4] = ["unit", "property", "proof", "conformance"];

pub fn run_tier(tier: &str, v: &Variant) -> TierResult {
    let t0 = Instant::now();
    let mut r = match tier {
        "unit" => unit_tier(v),
        "property" => property_tier(v, 1_000),
        "proof" => proof_tier(v),
        "conformance" => conformance_tier(v, 1_000),
        _ => unreachable!(),
    };
    r.seconds = t0.elapsed().as_secs_f64();
    r
}
