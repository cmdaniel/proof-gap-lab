"""The critical core: a ledger transfer with a 25 bps fee, written in the DSL.

Money is integer pence. A transfer moves `amount` from src to dst and charges
a fee of ceil(amount * 25 / 10000) to src. Invalid transfers are no-ops.

The *promises* are the properties we want mathematical certainty about.
They are deliberately the safety promises a regulator would ask about, and
deliberately not a full functional spec: that gap is part of the experiment.
"""

from __future__ import annotations

from .dsl import CeilDiv, FloorDiv, Program, Var, all_of, eq, ge, implies, ite, le, lt

MAX_BALANCE = 10**12          # £10bn in pence
FEE_BPS = 25

src, dst, amount = Var("src"), Var("dst"), Var("amount")
new_src, new_dst, fee_out = Var("new_src"), Var("new_dst"), Var("fee")
fee, ok = Var("fee_due"), Var("ok")

DOMAIN = {
    "src": (0, MAX_BALANCE),
    "dst": (0, MAX_BALANCE),
    "amount": (-MAX_BALANCE, MAX_BALANCE),
}

PROMISES = {
    # Money is neither created nor destroyed (fee goes to the bank).
    "conservation": eq(new_src + new_dst + fee_out, src + dst),
    # The payer never goes overdrawn.
    "no_overdraft": ge(new_src, 0),
    # Fees are never negative and the payee never loses money.
    "fee_non_negative": ge(fee_out, 0),
    "payee_never_debited": ge(new_dst, dst),
    # A non-positive amount changes nothing.
    "invalid_is_noop": implies(le(amount, 0),
                               all_of(eq(new_src, src), eq(new_dst, dst), eq(fee_out, 0))),
}


def transfer_program(**overrides) -> Program:
    """Build the transfer spec. `overrides` replace named lets/outputs, which
    is how the experiment injects bugs into the *source*."""
    lets = {
        "fee_due": CeilDiv(amount * FEE_BPS, 10_000),
        "ok": all_of(lt(0, amount), le(amount + fee, src)),
    }
    outs = {
        "new_src": ite(ok, src - (amount + fee), src),
        "new_dst": ite(ok, dst + amount, dst),
        "fee": ite(ok, fee, 0),
    }
    extra_lets = {}
    for k, v in overrides.items():
        if k in lets:
            lets[k] = v
        elif k in outs:
            outs[k] = v
        else:
            extra_lets[k] = v
    return Program(
        name="transfer",
        inputs=dict(DOMAIN),
        # New helper lets go first, so they may only depend on inputs.
        lets=tuple({**extra_lets, **lets}.items()),
        outputs=tuple(outs.items()),
        promises=dict(PROMISES),
    )


SPEC = transfer_program()


# --- Bugs injected into the source (the thing the proof *does* see) --------

def spec_fee_not_debited() -> Program:
    """Payer is charged the amount but not the fee: money is created."""
    return transfer_program(new_src=ite(ok, src - amount, src))


def spec_fee_helper_drift() -> Program:
    """The affordability check reuses the quote-screen fee helper (floor
    rounding, for display) while the debit uses the ledger fee (ceiling).
    Every fee charged is correct. But when the payer holds exactly
    amount + ledger_fee - 1 pence, the check passes and the debit takes them
    to -1p. One balance per amount triggers it."""
    quote_fee = Var("quote_fee")
    return transfer_program(
        quote_fee=FloorDiv(amount * FEE_BPS, 10_000),
        ok=all_of(lt(0, amount), le(amount + quote_fee, src)),
    )


HIGH_VALUE = 100_000_000  # £1m in pence


def spec_fee_helper_drift_high_value() -> Program:
    """Same drift, but only on the high-value path: payments of £1m or more
    get an indicative fee from the quote service (floor), everything else
    uses the ledger fee (ceiling). Same 1p overdraft, in a much smaller
    corner of the input space."""
    quote_fee = Var("quote_fee")
    return transfer_program(
        quote_fee=ite(ge(amount, HIGH_VALUE),
                      FloorDiv(amount * FEE_BPS, 10_000),
                      CeilDiv(amount * FEE_BPS, 10_000)),
        ok=all_of(lt(0, amount), le(amount + quote_fee, src)),
    )


def spec_fee_to_payee() -> Program:
    """The fee is credited to the payee instead of the bank. Every stated
    promise still holds: the proof is about what you wrote down, not what
    the business meant."""
    return transfer_program(
        new_dst=ite(ok, dst + amount + fee, dst),
        fee=ite(ok, 0, 0),
    )
