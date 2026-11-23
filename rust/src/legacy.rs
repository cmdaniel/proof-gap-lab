//! Existing "enterprise" Rust: the same business rule, written by hand.
//! A port of `python/src/proofgap/legacy.py`, including its float fee maths.
//!
//! Not written in the proof language, so the checker cannot prove anything
//! about it. The best we can do is test that it conforms to the proved spec.

#[derive(Debug)]
pub enum TransferRejected {
    NonPositiveAmount,
    InsufficientFunds,
}

#[derive(Debug)]
pub struct Account {
    pub id: String,
    pub balance_pence: i64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LedgerService {
    /// Reproduces a real-looking bug: someone "tightened" the check.
    pub strict_affordability: bool,
}

impl LedgerService {
    pub fn quote_fee(&self, amount_pence: i64) -> i64 {
        ((amount_pence * 25) as f64 / 10_000.0).ceil() as i64
    }

    pub fn can_afford(&self, account: &Account, amount_pence: i64, fee: i64) -> bool {
        if self.strict_affordability {
            amount_pence + fee < account.balance_pence
        } else {
            amount_pence + fee <= account.balance_pence
        }
    }

    pub fn transfer(
        &self,
        payer: &mut Account,
        payee: &mut Account,
        amount_pence: i64,
    ) -> Result<i64, TransferRejected> {
        if amount_pence <= 0 {
            return Err(TransferRejected::NonPositiveAmount);
        }
        let fee = self.quote_fee(amount_pence);
        if !self.can_afford(payer, amount_pence, fee) {
            return Err(TransferRejected::InsufficientFunds);
        }
        payer.balance_pence -= amount_pence + fee;
        payee.balance_pence += amount_pence;
        Ok(fee)
    }

    /// Adapter: present the legacy service with the spec's signature.
    pub fn as_function(&self, src: i64, dst: i64, amount: i64) -> (i64, i64, i64) {
        let mut a = Account {
            id: "a".into(),
            balance_pence: src,
        };
        let mut b = Account {
            id: "b".into(),
            balance_pence: dst,
        };
        let fee = self.transfer(&mut a, &mut b, amount).unwrap_or(0);
        (a.balance_pence, b.balance_pence, fee)
    }
}
