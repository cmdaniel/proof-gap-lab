//! The critical core: a ledger transfer with a 25 bps fee, written in the DSL.
//! A line-for-line port of `python/src/proofgap/spec.py`.
//!
//! Money is integer pence. A transfer moves `amount` from src to dst and
//! charges a fee of ceil(amount * 25 / 10000) to src. Invalid transfers are
//! no-ops. Shared with `build.rs`, so std only.

use crate::dsl::*;

pub const MAX_BALANCE: i64 = 1_000_000_000_000; // £10bn in pence
pub const FEE_BPS: i64 = 25;
pub const HIGH_VALUE: i64 = 100_000_000; // £1m in pence

fn src() -> Expr {
    var("src")
}
fn dst() -> Expr {
    var("dst")
}
fn amount() -> Expr {
    var("amount")
}
fn fee() -> Expr {
    var("fee_due")
}
fn ok() -> Expr {
    var("ok")
}

pub fn domain() -> Vec<(String, (i64, i64))> {
    vec![
        ("src".into(), (0, MAX_BALANCE)),
        ("dst".into(), (0, MAX_BALANCE)),
        ("amount".into(), (-MAX_BALANCE, MAX_BALANCE)),
    ]
}

pub fn promises() -> Vec<(String, Expr)> {
    let (new_src, new_dst, fee_out) = (var("new_src"), var("new_dst"), var("fee"));
    vec![
        // Money is neither created nor destroyed (fee goes to the bank).
        (
            "conservation".into(),
            eq(&new_src + &new_dst + &fee_out, src() + dst()),
        ),
        // The payer never goes overdrawn.
        ("no_overdraft".into(), ge(&new_src, 0)),
        // Fees are never negative and the payee never loses money.
        ("fee_non_negative".into(), ge(&fee_out, 0)),
        ("payee_never_debited".into(), ge(&new_dst, dst())),
        // A non-positive amount changes nothing.
        (
            "invalid_is_noop".into(),
            implies(
                le(amount(), 0),
                all_of(vec![
                    eq(&new_src, src()),
                    eq(&new_dst, dst()),
                    eq(&fee_out, 0),
                ]),
            ),
        ),
    ]
}

/// Build the transfer spec. `overrides` replace named lets/outputs; unknown
/// names become new helper lets placed first (so they may use inputs only).
pub fn transfer_program(overrides: Vec<(&str, Expr)>) -> Program {
    let mut lets: Vec<(String, Expr)> = vec![
        ("fee_due".into(), ceil_div(amount() * FEE_BPS, 10_000)),
        (
            "ok".into(),
            all_of(vec![lt(0, amount()), le(amount() + fee(), src())]),
        ),
    ];
    let mut outs: Vec<(String, Expr)> = vec![
        (
            "new_src".into(),
            ite(ok(), src() - (amount() + fee()), src()),
        ),
        ("new_dst".into(), ite(ok(), dst() + amount(), dst())),
        ("fee".into(), ite(ok(), fee(), 0)),
    ];
    let mut extra = vec![];
    for (k, v) in overrides {
        if let Some(slot) = lets.iter_mut().find(|(n, _)| n == k) {
            slot.1 = v;
        } else if let Some(slot) = outs.iter_mut().find(|(n, _)| n == k) {
            slot.1 = v;
        } else {
            extra.push((k.to_string(), v));
        }
    }
    extra.extend(lets);
    Program {
        name: "transfer".into(),
        inputs: domain(),
        lets: extra,
        outputs: outs,
        promises: promises(),
    }
}

pub fn spec() -> Program {
    transfer_program(vec![])
}

// --- Bugs injected into the source (the thing the proof *does* see) --------

/// Payer is charged the amount but not the fee: money is created.
pub fn spec_fee_not_debited() -> Program {
    transfer_program(vec![("new_src", ite(ok(), src() - amount(), src()))])
}

/// Affordability check uses the floor-rounded quote fee, debit uses the
/// ceiling ledger fee: 1p overdraft when balance == amount + fee - 1.
pub fn spec_fee_helper_drift() -> Program {
    transfer_program(vec![
        ("quote_fee", floor_div(amount() * FEE_BPS, 10_000)),
        (
            "ok",
            all_of(vec![
                lt(0, amount()),
                le(amount() + var("quote_fee"), src()),
            ]),
        ),
    ])
}

/// The same drift, only on the high-value (>= £1m) path.
pub fn spec_fee_helper_drift_high_value() -> Program {
    transfer_program(vec![
        (
            "quote_fee",
            ite(
                ge(amount(), HIGH_VALUE),
                floor_div(amount() * FEE_BPS, 10_000),
                ceil_div(amount() * FEE_BPS, 10_000),
            ),
        ),
        (
            "ok",
            all_of(vec![
                lt(0, amount()),
                le(amount() + var("quote_fee"), src()),
            ]),
        ),
    ])
}

/// Fee credited to the payee, not the bank. Every stated promise still holds.
pub fn spec_fee_to_payee() -> Program {
    transfer_program(vec![
        ("new_dst", ite(ok(), dst() + amount() + fee(), dst())),
        ("fee", ite(ok(), 0, 0)),
    ])
}
