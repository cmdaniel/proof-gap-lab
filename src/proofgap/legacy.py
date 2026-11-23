"""Existing "enterprise" Python: the same business rule, written by hand.

This is the code that actually runs in most organisations. It is not written
in the proof language, so the checker cannot prove anything about it. The
best we can do is *test that it conforms* to the proved spec.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

FEE_RATE = 0.0025


class TransferRejected(Exception):
    pass


@dataclass
class Account:
    id: str
    balance_pence: int


class LedgerService:
    def __init__(self, strict_affordability: bool = False):
        # Flag reproduces a real-looking bug: someone "tightened" the check.
        self.strict_affordability = strict_affordability

    def quote_fee(self, amount_pence: int) -> int:
        return math.ceil(amount_pence * 25 / 10_000)

    def can_afford(self, account: Account, amount_pence: int, fee: int) -> bool:
        if self.strict_affordability:
            return amount_pence + fee < account.balance_pence
        return amount_pence + fee <= account.balance_pence

    def transfer(self, payer: Account, payee: Account, amount_pence: int) -> int:
        if amount_pence <= 0:
            raise TransferRejected("amount must be positive")
        fee = self.quote_fee(amount_pence)
        if not self.can_afford(payer, amount_pence, fee):
            raise TransferRejected("insufficient funds")
        payer.balance_pence -= amount_pence + fee
        payee.balance_pence += amount_pence
        return fee


def as_function(service: LedgerService):
    """Adapter: present the legacy service with the spec's signature."""
    def transfer(src: int, dst: int, amount: int) -> tuple[int, int, int]:
        a, b = Account("a", src), Account("b", dst)
        try:
            fee = service.transfer(a, b, amount)
        except TransferRejected:
            fee = 0
        return a.balance_pence, b.balance_pence, fee
    return transfer
