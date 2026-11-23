# proof-gap-lab

A small, runnable experiment on where formal verification stops and tests have to take over, built twice: once in **Python** and once in **Rust**. Each version uses a toy proof language, the Z3 solver and a property-testing library to measure which bugs a proof catches, which ones it cannot see, and which tests close the gap. A comparison script then checks that the two languages reach the same conclusions and measures how fast each one runs.

> **Educational experiment, not production software.** Nothing here is affiliated with or endorsed by the Bend, Z3, Hypothesis or proptest projects. Bend 2 is the motivating example, not the subject: this lab does not run or audit Bend. See [Limits](#limits) and [Conditions of use](#conditions-of-use).

It tests two claims:

1. **The compiler verification gap.** In proof languages such as Bend 2, the proof-checking core can be trustworthy while the compiler that turns source into an executable is not. A proof about the source is not a proof about the binary. And a proof language only covers code written in it, so existing TypeScript, Python or Rust services are outside its reach.
2. **Verification is the top tier, not a replacement.** Formal proof gives certainty about the core promises across *every* input, which path-based tests cannot reach. But it only proves what you wrote down, about the code it can see. Conventional tests still do work that proofs cannot.

## What is formal verification?

**Formal verification means using mathematics, checked by a machine, to show that a program meets a precise specification for every possible input, not just the inputs someone thought to try.**

A test asks: *"For this input, is the output right?"* A proof asks: *"Is there any input at all for which the output is wrong?"* If the answer is a machine-checked "no", the property holds everywhere.

It always has three parts:

| Part | What it is | In this lab |
|---|---|---|
| **Specification** | Precise statements of what must always be true | The five core promises, e.g. *the payer is never overdrawn*: `new_src ≥ 0` |
| **Model of the code** | The program in a form a machine can reason about | The transfer written in the lab's small DSL |
| **Checker** | A tool that either proves the specification or produces a counterexample | The Z3 SMT solver |

Here is the difference in scale. The transfer takes three numbers, each ranging up to £10bn in pence: about 2 × 10³⁶ possible inputs. A thousand random tests check a thousand of them. Z3 does not enumerate inputs at all. It reasons symbolically about all of them at once, and answers in milliseconds with one of three results:

- **Proved:** no input in the whole range breaks the promise.
- **Counterexample:** a concrete input that breaks it, e.g. `src=100250399, amount=100000399` leaves the payer at −1p.
- **Unknown:** the solver gave up within its time limit. That is not a pass.

There are several families of tools. They include SMT solvers such as Z3 (used here), proof assistants such as Lean and Rocq, model checkers, and languages whose type checker is a proof checker, such as Bend 2. They all share the same promise and the same limit: **certainty about exactly what was specified, for exactly the code the checker saw**. The rest of this lab is about that limit.

## Why it matters for GenAI workflows

Generative AI changes the economics of writing code. A model can produce a plausible-looking function in seconds, but plausible is not correct, and the bottleneck moves from *writing* code to *trusting* it. Formal verification is one of the few ways to build that trust without a human reading every line:

- **An independent judge.** A test suite written by the same model that wrote the code can share its blind spots: it tends to check what the code *does*, not what it *should* do. A proof checker does not care how the code was produced or how convincing it looks. It accepts or rejects. That is the idea behind proof-oriented languages like Bend 2, which describes itself as blocking AI mistakes via proof.
- **Rare-path bugs are exactly what models produce.** Generated code tends to handle the common case well. Its mistakes hide in edge cases and special branches, such as the ≥ £1m path in this lab, which neither the example tests, 100,000 property tests nor 1 billion random samples found. A proof covers those paths by construction.
- **Precise feedback for agents.** A failed proof returns a concrete counterexample, not a vague "test failed". An agent can feed `(src=100250399, amount=100000399) → overdraft` straight back into its next attempt and repair loop, until the checker accepts.
- **A stable gate for unstable generators.** Model outputs change between runs and between model versions. The specification and the checker don't, so regenerated code is held to the same bar every time.
- **Humans move up a level.** Reviewing five short promises is far easier than reviewing every generated line. People own the intent, and the machine checks the implementation against it.
- **Evidence for regulated settings.** A proof plus its specification is an auditable artifact, which matters in finance, healthcare and safety.

This lab's results also show where GenAI makes the gaps *more* dangerous, not less:

| GenAI risk | What the lab shows |
|---|---|
| The model writes, or quietly weakens, the specification | *fee paid to payee* is fully proved and still wrong. If the same model writes the promises and the code, the proof can certify the wrong behaviour. **Humans must own the spec**, and example tests must capture intent. |
| Generated code goes into mainstream languages that cannot be proved | The *legacy* services. Most generated Python, TypeScript or Rust sits outside any proof, so test it against the proved spec (conformance). |
| Generated code goes through compilers, code generators and glue | The two *compiler* bugs pass the proof. Whatever sits between the proof and the running code needs its own tests. |
| Fast generation tempts teams to skip slower checks | Here the proof takes about 4 ms, and all four tiers together take 0.2 s in Rust or 3.5 s in Python. Skipping any layer saves almost nothing, and each one catches bugs the others miss. |

A workflow that puts these together looks like this:

```
 human: intent + core promises (the spec)
                  │
                  ▼
       model generates the code  ◄──────────────┐
                  │                              │
                  ▼                              │
       checker (e.g. Z3): proved? ── no ── counterexample
                  │ yes
                  ▼
   conformance + property + example tests on the code that actually runs
                  │
                  ▼
                ship
```

> **Note:** this lab contains no language model. It isolates the verification half of that loop, the checker, the gap and the tests around it, so each part can be measured on its own.

## The business problem

Teams that handle money, safety or regulated data are starting to adopt formal verification, often alongside AI-generated code and proof-oriented languages such as Bend 2. That raises a practical question for engineering and risk leads:

> **If a critical rule is mathematically proved, which tests can we drop, and which failures can still reach customers?**

There are two ways to get this wrong:

- **Over-trusting the proof.** "It's proved, ship it", even though the proof never saw the compiler, the hand-written legacy service that actually runs in production, or a business rule that nobody wrote down as a promise.
- **Under-using the proof.** Relying on example and random tests for rules where a single rare failure is a reportable incident, even though tests can never cover every input.

In payments, a defect that creates 1p, overdraws one account or misroutes one fee is small on its own. At scale it becomes a reconciliation break, a revenue leak or a conduct issue. The decision this lab supports is **which assurance activity to rely on for which kind of defect, what each one costs, and whether choosing a faster language changes the answer.**

## The use case under test

A ledger **transfer** service, the kind of small, critical core found in every payments platform:

- It moves `amount` pence from a payer to a payee.
- It charges a **25 basis point fee** (0.25%), rounded **up** to the next penny, which goes to the bank.
- It rejects transfers with a non-positive amount, or where the payer cannot cover amount plus fee. A rejected transfer changes nothing.

For example, a £100 transfer from a £500 balance leaves the payer with £399.75, credits the payee £100 and collects 25p for the bank.

The rules split into two groups, and the split matters:

| Kind of rule | Rules | How it is checked |
|---|---|---|
| **Core promises** (written down and proved) | money is conserved · the payer is never overdrawn · the fee is never negative · the payee is never debited · an invalid amount changes nothing | Z3 proof over every input up to £10bn |
| **Business intent** (true, but not written as a promise) | the bank, not the payee, receives the fee · fees round up, not down · a customer can spend their entire balance | only example tests and conformance can see these |

The code exists in three forms, as it does in most organisations: the **specification** that gets proved, the **compiled executable** produced from it by an unverified compiler, and an **existing hand-written service** that implements the same rule without any proof.

Seven realistic defects are injected, each modelled on a familiar kind of production incident:

| Injected defect | Where | Real-world analogue | Business impact |
|---|---|---|---|
| fee not debited | spec | a refactor drops part of a debit | money created; ledger does not reconcile |
| fee helper drift | spec | affordability check and debit use two different fee helpers | customers overdrawn by 1p |
| fee helper drift, ≥ £1m only | spec | the same drift, but only on the high-value payments path | rare overdrafts on the largest customers; very hard to find by testing |
| fee paid to payee | spec | the fee is credited to the wrong party | revenue loss, yet every stated promise still holds |
| compiler: associativity | compiler | a toolchain or code-generator change | balances computed wrongly for every paid transfer |
| compiler: ceil template | compiler | a rounding regression in generated code | systematic 1p undercharge, invisible to safety checks |
| legacy: strict check | legacy | `<` instead of `<=` in a hand-written service | customers cannot spend their full balance; complaints |

## Expected outcomes

The lab tests six hypotheses. Each one is checked against the detection matrix in both Python and Rust.

| # | Hypothesis | Outcome |
|---|---|---|
| H1 | A proof catches rare spec defects that example and random tests miss. | **Confirmed.** Only the proof catches the ≥ £1m drift: 4 ms, versus a miss after 100,000 property tests and 1 billion random samples. |
| H2 | A proof is blind to defects introduced by the compiler. | **Confirmed.** Both compiler bugs pass the proof. Conformance testing against the proved semantics catches them. |
| H3 | A proof cannot cover existing code that is not written in the proof language. | **Confirmed.** The proof tier is not applicable to the legacy service. Only conformance catches its `<` bug. |
| H4 | A proof only covers the promises you wrote down. | **Confirmed.** "Fee paid to payee" is fully proved. Only a business example test catches it. |
| H5 | Conformance testing cannot catch a wrong specification. | **Confirmed.** It misses every spec defect, because the executable faithfully implements the wrong spec. |
| H6 | A faster language makes testing cheaper but changes neither the cost of proof nor what a proof guarantees. | **Confirmed.** Rust tests are 60–340× faster than Python, while proof time is about 4 ms in both. |

**What the outcome means for a team:** no single layer is enough, and each one has a job no other layer does:

- **Proof:** certainty for the few promises where any failure is unacceptable.
- **Conformance testing:** keeps the compiled and legacy code faithful to what was proved.
- **Property testing:** cheap, broad coverage of the code that actually runs.
- **Example tests:** the only layer that captures business intent nobody formalised.

The [Results](#results) section shows the evidence for each hypothesis.

## Repository layout

```
proof-gap-lab/
├── python/                 Python lab (uv project)
│   ├── src/proofgap/       dsl, spec, checker, compiler, legacy, tiers, experiment, bench
│   ├── tests/              pytest suite pinning the results
│   ├── RESULTS.md          generated: matrix + evidence + emitted code
│   ├── results.json        generated: machine-readable results
│   └── bench.json          generated: performance numbers
├── rust/                   Rust lab (cargo crate), same modules, same experiment
│   ├── build.rs            runs the unverified DSL → Rust compiler at build time
│   ├── src/                dsl, spec, catalog, checker, compiler, legacy, tiers
│   ├── src/bin/            experiment.rs, bench.rs
│   ├── tests/lab.rs        cargo test suite pinning the results
│   └── RESULTS.md, results.json, bench.json
├── scripts/compare.py      cross-language check → COMPARISON.md
└── COMPARISON.md           generated: Python vs Rust, side by side
```

## The setup

Both languages build the same stack around one critical function, a ledger transfer with a 25 basis point fee.

| Real-world piece | Python | Rust |
|---|---|---|
| Proof language source | [`spec.py`](python/src/proofgap/spec.py) in a tiny integer DSL ([`dsl.py`](python/src/proofgap/dsl.py)) | [`spec.rs`](rust/src/spec.rs) in the same DSL ([`dsl.rs`](rust/src/dsl.rs)) |
| Verified proof-checking core | [`checker.py`](python/src/proofgap/checker.py): DSL → Z3 | [`checker.rs`](rust/src/checker.rs): DSL → Z3, building the same terms |
| Unverified compiler to executable | [`compiler.py`](python/src/proofgap/compiler.py): DSL → Python source → `exec` | [`compiler.rs`](rust/src/compiler.rs), run by [`build.rs`](rust/build.rs): DSL → Rust source → `rustc` → native code |
| Existing enterprise code | [`legacy.py`](python/src/proofgap/legacy.py): hand-written `LedgerService` | [`legacy.rs`](rust/src/legacy.rs): the same service, hand-written in Rust |

**Core promises**, proved for all `src, dst ∈ [0, £10bn]` and `amount ∈ [−£10bn, £10bn]` in pence: money is conserved, the payer is never overdrawn, the fee is never negative, the payee is never debited, and invalid amounts change nothing.

**Four tiers**, in [`tiers.py`](python/src/proofgap/tiers.py) and [`tiers.rs`](rust/src/tiers.rs):

| Tier | What it runs against | Oracle |
|---|---|---|
| unit | the executable | 5 hand-written examples from the business requirements |
| property | the executable | the core promises, on 1,000 random inputs (Hypothesis in Python, proptest in Rust) |
| proof | the DSL **source** | the core promises, for **every** input (Z3) |
| conformance | the executable | exact output equality with the *proved* Z3 semantics, on solver-generated boundary inputs plus 1,000 random ones |

## Results

Both languages produce the same detection matrix except for one property-testing cell. Each language's full evidence and generated code are in [python/RESULTS.md](python/RESULTS.md) and [rust/RESULTS.md](rust/RESULTS.md). The side-by-side view is in [COMPARISON.md](COMPARISON.md).

| variant | bug lives in | unit | property | proof | conformance |
|---|---|---|---|---|---|
| baseline (compiled) | - | · | · | · | · |
| fee not debited | source | **CAUGHT** | **CAUGHT** | **CAUGHT** | · |
| fee helper drift | source | · | **Python only** | **CAUGHT** | · |
| fee helper drift, ≥ £1m only | source | · | · | **CAUGHT** | · |
| fee paid to payee | source | **CAUGHT** | · | · | · |
| compiler: associativity | compiler | **CAUGHT** | **CAUGHT** | · | **CAUGHT** |
| compiler: ceil template | compiler | **CAUGHT** | · | · | **CAUGHT** |
| legacy service | legacy | · | · | n/a | · |
| legacy: strict check | legacy | · | · | n/a | **CAUGHT** |

`CAUGHT` = the tier reports a failure. `·` = the tier passes and the bug gets through. `n/a` = the tier cannot be applied.

Each of the upper three tiers catches at least one bug that nothing else catches, in both languages.

### 1. The proof reaches what sampling cannot

*fee helper drift, ≥ £1m only*: on the high-value path, the affordability check uses a floor-rounded quote fee, but the debit takes the ceiling ledger fee. When the payer holds exactly `amount + fee − 1` pence, they end up at −1p. Each amount has only one balance that triggers it, and only above £1m.

- Z3 refutes `no_overdraft` in about **4 ms** in both languages, with the same counterexample: `src=100250399, amount=100000399`.
- Property testing misses it at 100,000 inputs in both languages: 23 s in Python, 0.07 s in Rust.
- Raw random sampling in Rust tried **1 billion** inputs in 6 s and still missed. The exact chance of hitting the bug with one random input is 4.97 × 10⁻¹³, so random sampling needs about 2 × 10¹² tries on average: about 3.5 hours in Rust, or about 34 days in Python.

Speed changes how much sampling you can afford. It does not turn sampling into a guarantee. The proof is still faster than the fastest sampler by six orders of magnitude, and it also proves that every *other* input is safe.

### 2. The compiler gap is real, and the proof cannot see it

Both compiler bugs are one-character slips in an otherwise reasonable code generator. The Rust version emits real Rust that `rustc` compiles:

```rust
// correct emit                                     // assoc_bug: '-' treated as associative
let new_src = if ok { src - (amount + fee_due) } …   let new_src = if ok { src - amount + fee_due } …
```

For both bugs, the proof tier reports *all 5 promises proved*, which is true of the source. The executable is still wrong. The *ceil template* bug is the more dangerous one: every safety promise still holds at runtime (fees are just 1p low), so property tests pass too. Only exact-output checks catch it.

**Conformance testing is the bridge across the gap.** It checks that the code that runs computes what the code that was proved computes. Z3 also picks the test inputs: for every decision boundary in the spec, it generates inputs on and next to the edge. In both languages, the first such input, `(2, 0, 1)`, exposed both compiler bugs and the legacy off-by-one.

### 3. Legacy code is outside the proof's reach

The hand-written `LedgerService` cannot be proved in either language, because it is not in the proof language. The *strict check* bug (`<` where `<=` was meant, so a customer can never spend their whole balance) breaks no safety promise, and the typical unit suite never tries a full-balance payment. Only conformance against the proved spec catches it.

That is the practical path for existing codebases: prove the specification, then test the existing code against it.

### 4. A proof only covers what you stated

*fee paid to payee* credits the fee to the recipient instead of the bank. Every promise is proved, and conformance passes too, because the executable faithfully implements the wrong spec. Only a plain unit test, written from what the business actually wanted ("the bank collects 25p"), fails. The result is the same in both languages.

## Python vs Rust: do the results match?

[`scripts/compare.py`](scripts/compare.py) reads both `results.json` files and both `bench.json` files, writes [COMPARISON.md](COMPARISON.md), and with `--check` fails CI if the deterministic tiers disagree.

| Check | Result |
|---|---|
| Same Z3 in both languages | Yes: 5.4.1. Python uses `z3-solver`; Rust's `z3-sys` downloads the same release (pinned in [`rust/.cargo/config.toml`](rust/.cargo/config.toml)) |
| Unit, proof and conformance tiers | **Agree on all 27 cells** |
| Property tier | Agrees on 8 of 9 variants (see below) |
| Proof counterexamples and solver-chosen boundary inputs | **Identical** in every case, e.g. `(100250399, 0, 100000399)` |
| Compiled transfer, 1,000,000 identical inputs | Same output checksum in both languages: `3e348fe7fba5dad5` |
| Hand-written legacy service, same inputs | Same checksum as the compiled code, in both languages |
| Z3 oracle vs compiled code, 2,000 inputs | Agree in both languages, and the oracle checksums match across languages |

The cross-check is strong because the two languages were built independently from the same DSL: two compilers, two hand-written services and two Z3 bindings. All four executables agree on a million inputs, and both proof pipelines return the same counterexamples.

### Where they differ, and why

- **Property testing: library, not language.** Hypothesis catches *fee helper drift* (all amounts) at `src=1, amount=1`, because it deliberately generates small and repeated values. For small amounts the bug triggers whenever `src == amount`. proptest samples integer ranges uniformly, so with 1,000 cases it never lands on an exact balance. A different library search strategy changed a result. That is one more reason not to treat any single sampling tier as a guarantee.
- **Shrinking.** Hypothesis shrinks failures to minimal examples like `(2, 0, 1)`. proptest stopped at `(193343556457, 0, 1)`. Both are valid witnesses, but the Python one is easier to read.
- **Integer semantics.** The proof uses mathematical integers. Python's integers are unbounded, so they match. Rust uses `i64`, so the Rust checker adds **overflow obligations**: Z3 proves that all 25 integer subterms of the spec fit in `i64` for every input in the domain. This closes one compiler gap that only exists in Rust.
- **Division.** Rust's `/` truncates toward zero, while the DSL, Python's `//` and Z3 all floor. The Rust emitter uses `div_euclid`. A naive port that emitted `/` would have been a third compiler bug, and here a silent one: fee numerators are never negative for valid transfers, so no test would ever see it.
- **Where the compiler runs.** Python compiles generated source at runtime with `exec`. Rust generates source in `build.rs`, and then `rustc` compiles it. That places a second, much larger unverified compiler (`rustc` and LLVM) between the proof and the machine. It is very well tested, but not verified, which is exactly the point of claim 1.

## Performance

One run on Apple Silicon (macOS, arm64), Rust in release mode. These are medians of repeated runs. Read the ratios, not the absolute numbers. Full numbers are in [COMPARISON.md](COMPARISON.md).

| workload | Python | Rust | Rust advantage |
|---|---|---|---|
| compiled transfer, per call | 446 ns | 1.5 ns | ~300× |
| hand-written legacy service, per call | 600 ns | 9.5 ns | ~60× |
| property tier, 1,000 cases | 205 ms | 0.8 ms | ~270× |
| property tier, 100,000 cases | 23.1 s | 0.07 s | ~340× |
| raw random sampling | 0.7 M/s | 162 M/s | ~235× |
| Z3 oracle evaluation, per call | 98 µs | 23 µs | ~4× |
| solver boundary inputs (14 witnesses) | 7.3 ms | 5.1 ms | ~1.4× |
| prove all 5 promises | 3.7 ms | 4.0 ms | ~1× |
| whole experiment (9 variants × 4 tiers) | ~3.5 s | ~0.2 s | ~17× |

What the numbers show:

- **Testing is where the language matters.** Executing and sampling are 60–340× faster in Rust, so Rust can afford far more random and conformance testing in the same CI budget.
- **Proving is where it does not.** Both languages hand the same question to the same C++ solver, so proof time is the same (about 4 ms). The Python binding overhead is too small to see here.
- **Oracle calls are in between.** Each one crosses into Z3, so the gap shrinks to about 4×.
- **The proof's advantage does not depend on the language.** Proving or refuting all five promises takes about 4 ms. Finding the high-value bug by random sampling takes hours even at native speed.

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
- Treat everything between the proof and the running code (compiler, runtime, machine integers, glue, legacy) as unverified. Test it against the proved semantics, using the solver to pick inputs, and discharge machine-level obligations such as overflow where you can.
- Keep example tests. They are the only tier here that encodes *intent* rather than stated properties.
- A faster language buys more testing, not certainty.

## Trusted base, honestly

Even the "verified core" relies on code that is not verified: Z3 itself, the `to_z3` translation in each language, and the oracles that evaluate proved terms. In Rust, that also includes `rustc` and LLVM. Both test suites do the obvious cross-check: every counterexample the solver returns is replayed on the compiled code and must reproduce. The Python–Rust comparison adds a further independent check: two separate implementations agree on the same counterexamples and on a million outputs. Every proof has a trusted base. The useful question is how small it is and what you test around it.

## Limits

- It is an analogy, not a measurement of any real proof language. The Bend 2 framing describes the general architecture of proof-carrying languages. It is not a statement about Bend 2's current compiler or its verification status, which can change between releases.
- It is a toy: one function, linear integer arithmetic (the fee rate is a constant, which keeps Z3 decidable and fast), and no floats in the spec, concurrency or I/O.
- The bugs were chosen to illustrate each layer. They are realistic, but the matrix depends on them. The unit suite is meant to be typical, and a different suite would change some cells (a "spend the whole balance" test would also catch *strict check*).
- Random testing is seeded (Hypothesis `derandomize=True`, proptest with a fixed ChaCha seed), so results are reproducible. Other seeds or library versions may find or miss different things.
- Performance numbers come from one machine and one run. CI runners and other machines will differ, and micro-benchmarks of a 1–500 ns function are sensitive to inlining and measurement overhead.
- Results were produced with z3-solver 5.4.1 and Hypothesis 6.170.17 on Python 3.13.14, and with Rust 1.98.1, the `z3` crate 0.25 (Z3 5.4.1) and proptest 1.12. Lock files pin them: [`python/uv.lock`](python/uv.lock) and `rust/Cargo.lock`. Newer versions may change counterexamples, timings or which random inputs get tried.

## Run it

```bash
# Python (needs uv, Python ≥ 3.11)
cd python
uv sync
uv run pytest -q                       # 15 tests: pins the matrix and the cross-checks
uv run python -m proofgap.experiment   # matrix → RESULTS.md, results.json (~4 s)
uv run python -m proofgap.bench        # performance → bench.json (~35 s)

# Rust (needs Rust ≥ 1.88; the first build downloads Z3 5.4.1 from GitHub releases)
cd ../rust
cargo test --release                   # 6 tests: same claims, plus i64 overflow obligations
cargo run --release --bin experiment   # matrix → RESULTS.md, results.json (~0.2 s)
cargo run --release --bin bench        # performance → bench.json (~10 s)

# Compare
cd ..
python3 scripts/compare.py             # → COMPARISON.md
python3 scripts/compare.py --check     # exit 1 if deterministic tiers disagree
```

CI ([`.github/workflows/test.yml`](.github/workflows/test.yml)) runs both test suites and both experiments, then runs the cross-language `--check` on every push.

## References

These links and descriptions were checked on **23 November 2026**. Projects move, rename and change their claims, so treat this list as a snapshot and check the source before relying on it.

- Bend 2: [github.com/bendlang/bend](https://github.com/bendlang/bend) · [bend-lang.com](https://bend-lang.com). Describes itself as "a fast language that blocks AI mistakes via proof". The original Bend 1 lives at [github.com/HigherOrderCO/Bend](https://github.com/HigherOrderCO/Bend).
- Z3 theorem prover: [github.com/Z3Prover/z3](https://github.com/Z3Prover/z3) · [Z3 guide](https://microsoft.github.io/z3guide/)
- Z3 Rust bindings: [github.com/prove-rs/z3.rs](https://github.com/prove-rs/z3.rs) · [docs.rs/z3](https://docs.rs/z3/0.25.0/z3/)
- Hypothesis (property-based testing for Python): [github.com/HypothesisWorks/hypothesis](https://github.com/HypothesisWorks/hypothesis) · [docs](https://hypothesis.readthedocs.io/)
- proptest (property-based testing for Rust): [github.com/proptest-rs/proptest](https://github.com/proptest-rs/proptest)
- uv (Python project manager used to run the Python lab): [github.com/astral-sh/uv](https://github.com/astral-sh/uv)

## Conditions of use

- Licensed under the [MIT License](LICENSE). The software is provided "as is", without warranty of any kind.
- For learning and discussion only. The ledger, fee rule and bugs are synthetic. They do not describe any real bank, product or incident, and they are not financial, legal or compliance advice.
- Do not treat a passing run of this lab as evidence that any real system is correct. The point of the experiment is that a proof covers only the promises you stated, about the code the checker can see.
- Performance figures are illustrative measurements from one machine, not a general benchmark of Python or Rust.
- Third-party names (Bend, Z3, Hypothesis, proptest, uv, Rust) belong to their owners. They are used here only to identify those projects.
