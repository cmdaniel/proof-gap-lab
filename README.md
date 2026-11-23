# proof-gap-lab

A small, runnable experiment on where formal verification stops and tests have to take over.

It tests two claims:

1. **The compiler verification gap.** In proof languages such as Bend 2, the proof-checking core can be trustworthy while the compiler that turns source into an executable is not. A proof about the source is not a proof about the binary. And a proof language only covers code written in it, so existing TypeScript or Python services are outside its reach.
2. **Verification is the top tier, not a replacement.** Formal proof gives certainty about the core promises across *every* input, which path-based tests cannot reach. But it only proves what you wrote down, about the code it can see. Conventional tests still do work that proofs cannot.

The lab builds a toy version of the whole stack around one critical function (a ledger transfer with a fee), injects realistic bugs at each layer, and records which reliability tier catches each one.

## The setup

| Real-world piece | In this lab |
|---|---|
| Proof language source | [`spec.py`](src/proofgap/spec.py): the transfer, written in a tiny integer DSL ([`dsl.py`](src/proofgap/dsl.py)) |
| Verified proof-checking core | [`checker.py`](src/proofgap/checker.py): DSL → Z3. Proves each promise for all inputs, or returns a counterexample |
| Unverified compiler to executable | [`compiler.py`](src/proofgap/compiler.py): DSL → Python source → callable, using a normal precedence-table emitter |
| Existing enterprise code | [`legacy.py`](src/proofgap/legacy.py): the same rule, written by hand as a `LedgerService` class |

**Core promises** proved over all `src, dst ∈ [0, £10bn]` and `amount ∈ [−£10bn, £10bn]`, in pence:
conservation of money, no overdraft, non-negative fee, payee never debited, and invalid amounts are no-ops.

**Four tiers**, all in [`tiers.py`](src/proofgap/tiers.py):

| Tier | What it runs against | Oracle |
|---|---|---|
| unit | the executable | 5 hand-written examples from the business requirements |
| property | the executable | the core promises, checked on 1,000 Hypothesis inputs |
| proof | the DSL **source** | the core promises, for **every** input (Z3) |
| conformance | the executable | output equality with the *proved* Z3 semantics, on solver-generated boundary inputs plus 1,000 random ones |

## Results

`uv run python -m proofgap.experiment` produces this table (full evidence and the generated code are in [RESULTS.md](RESULTS.md)):

| variant | bug lives in | unit | property | proof | conformance |
|---|---|---|---|---|---|
| baseline (compiled) | - | · | · | · | · |
| fee not debited | source | **CAUGHT** | **CAUGHT** | **CAUGHT** | · |
| fee helper drift | source | · | **CAUGHT** | **CAUGHT** | · |
| fee helper drift, ≥ £1m only | source | · | · | **CAUGHT** | · |
| fee paid to payee | source | **CAUGHT** | · | · | · |
| compiler: associativity | compiler | **CAUGHT** | **CAUGHT** | · | **CAUGHT** |
| compiler: ceil template | compiler | **CAUGHT** | · | · | **CAUGHT** |
| legacy service | legacy | · | · | n/a | · |
| legacy: strict check | legacy | · | · | n/a | **CAUGHT** |

Each of the upper three tiers catches at least one bug that nothing else catches.

### 1. The proof reaches what sampling cannot

*fee helper drift, ≥ £1m only*: on the high-value path, the affordability check uses a floor-rounded quote fee, but the debit takes the ceiling ledger fee. When the payer holds exactly `amount + fee − 1` pence, they end up at −1p. Each amount has only one balance that triggers it, and only above £1m.

- Z3 refutes `no_overdraft` in about **10 ms**: `src=100250399, amount=100000399`.
- Hypothesis misses it at 1,000 inputs, and still misses it at **100,000 inputs (23 s)**.

When the same drift applies to *all* amounts, Hypothesis finds it immediately (shrunk to `src=1, amount=1`): for small amounts, the trigger is just `src == amount`, and Hypothesis deliberately generates small and repeated values. Property testing is strong. It is just not exhaustive, and bugs that only fire under particular conditions are where that matters.

### 2. The compiler gap is real, and the proof cannot see it

Both compiler bugs are one-character slips in an otherwise reasonable code generator:

```python
# correct emit                              # assoc_bug: '-' treated as associative
new_src = src - (amount + fee_due) if ok …   new_src = src - amount + fee_due if ok …
```

For both, the proof tier reports *all 5 promises proved*, which is true of the source. The executable is still wrong. The *ceil template* bug is the more dangerous one: every safety promise still holds at runtime (fees are just 1p low), so property tests pass as well. Only exact-output checks catch it: the unit test with a 2.5p fee, and conformance testing against the proved semantics.

**Conformance testing is the bridge across the gap.** It checks that the thing that runs computes what the thing that was proved computes. Here, the solver also chooses the test inputs: for every decision boundary in the spec, Z3 generates inputs on and next to the edge. That found the compiler bugs, and the legacy off-by-one, on its first boundary input.

### 3. Legacy code is outside the proof's reach

The hand-written `LedgerService` cannot be proved. It is not in the proof language. The *strict check* bug (`<` where `<=` was meant, so a customer can never spend their whole balance) breaks no safety promise, so property tests pass, and the typical unit suite never tries a full-balance payment. Only conformance against the proved spec catches it.

That is the practical path for existing codebases: prove the specification, then test the existing code against it.

### 4. A proof only covers what you stated

*fee paid to payee* credits the fee to the recipient instead of the bank. Money is conserved, nobody is overdrawn, the fee is non-negative: **every promise is proved**. Conformance passes too, because the executable faithfully implements the wrong spec. Only a plain unit test, written from what the business actually wanted ("the bank collects 25p"), fails.

## What this suggests

```
            ┌──────────────────────────┐
            │  proof     (all inputs)  │  core safety promises, about the source
            ├──────────────────────────┤
            │  conformance             │  executable == proved semantics
            ├──────────────────────────┤
            │  property  (sampled)     │  promises on the real binary
            ├──────────────────────────┤
            │  unit      (examples)    │  intent: what the business meant
            └──────────────────────────┘
```

- Prove the few promises where being wrong is unacceptable, such as conservation and no overdraft.
- Treat everything between the proof and the running code (compiler, runtime, glue, legacy) as unverified. Test it against the proved semantics, using the solver to pick the inputs.
- Keep example tests. They are the only tier here that encodes *intent* rather than stated properties.

## Trusted base, honestly

Even this lab's "verified core" relies on code that is not verified: Z3 itself, the `to_z3` translation, and the `Oracle` that evaluates proved terms. The tests in [`tests/test_lab.py`](tests/test_lab.py) do the obvious cross-check: every counterexample the solver returns is replayed on the compiled code and must reproduce. Every proof has a trusted base. The useful question is how small it is and what you test around it.

## Limits

- It is a toy: one function, linear integer arithmetic (the fee rate is a constant, which keeps Z3 decidable and fast), and unbounded Python integers. Real targets add overflow, floats, concurrency and I/O.
- The bugs were chosen to illustrate each layer. They are realistic, but the matrix depends on them. The unit suite is meant to be typical, and a different suite would change some cells (a "spend the whole balance" test would also catch *strict check*).
- Hypothesis runs are derandomized, so results are reproducible. Other seeds may find or miss different things.

## Run it

```bash
uv sync
uv run python -m proofgap.experiment   # prints the matrix, writes RESULTS.md (~4 s)
uv run pytest -q                        # pins the matrix and the cross-checks
```

Requires Python ≥ 3.11. Dependencies: `z3-solver`, `hypothesis`, `pytest`.
