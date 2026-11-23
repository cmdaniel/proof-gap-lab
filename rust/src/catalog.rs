//! The compiled variants of the experiment, shared by `build.rs` (which
//! compiles them) and the library (which tests them). Same names and stories
//! as `python/src/proofgap/experiment.py`. Std only.

use crate::compiler::Bugs;
use crate::dsl::Program;
use crate::spec;

pub struct CompiledVariant {
    pub name: &'static str,
    pub layer: &'static str,
    pub story: &'static str,
    pub ident: &'static str,
    pub source: fn() -> Program,
    pub bugs: Bugs,
}

const NO_BUGS: Bugs = Bugs {
    assoc_bug: false,
    ceil_bug: false,
};

pub const COMPILED: &[CompiledVariant] = &[
    CompiledVariant {
        name: "baseline (compiled)",
        layer: "-",
        story: "Correct source, correct compiler.",
        ident: "baseline",
        source: spec::spec,
        bugs: NO_BUGS,
    },
    CompiledVariant {
        name: "fee not debited",
        layer: "source",
        story: "Payer charged the amount but not the fee; money is created.",
        ident: "fee_not_debited",
        source: spec::spec_fee_not_debited,
        bugs: NO_BUGS,
    },
    CompiledVariant {
        name: "fee helper drift",
        layer: "source",
        story: "Affordability check uses the floor-rounded quote fee, debit uses the ceiling ledger fee: \
                1p overdraft when balance == amount + fee - 1.",
        ident: "fee_helper_drift",
        source: spec::spec_fee_helper_drift,
        bugs: NO_BUGS,
    },
    CompiledVariant {
        name: "fee helper drift, ≥ £1m only",
        layer: "source",
        story: "The same drift, but only on the high-value (≥ £1m) path.",
        ident: "fee_helper_drift_high_value",
        source: spec::spec_fee_helper_drift_high_value,
        bugs: NO_BUGS,
    },
    CompiledVariant {
        name: "fee paid to payee",
        layer: "source",
        story: "Fee credited to the payee, not the bank. Every stated promise still holds.",
        ident: "fee_to_payee",
        source: spec::spec_fee_to_payee,
        bugs: NO_BUGS,
    },
    CompiledVariant {
        name: "compiler: associativity",
        layer: "compiler",
        story: "Codegen treats '-' as associative: a - (b + c) becomes a - b + c.",
        ident: "compiler_assoc",
        source: spec::spec,
        bugs: Bugs {
            assoc_bug: true,
            ceil_bug: false,
        },
    },
    CompiledVariant {
        name: "compiler: ceil template",
        layer: "compiler",
        story: "Misplaced parenthesis turns ceiling division into floor division.",
        ident: "compiler_ceil",
        source: spec::spec,
        bugs: Bugs {
            assoc_bug: false,
            ceil_bug: true,
        },
    },
];
