//! Performance benchmarks, same workloads and input stream as
//! `python/src/proofgap/bench.py`, so checksums must match.
//!
//!     cargo run --release --bin bench          # writes bench.json

use std::hint::black_box;
use std::time::Instant;

use proofgap::checker::{Oracle, Status, boundary_witnesses, prove};
use proofgap::generated::{baseline, fee_helper_drift_high_value};
use proofgap::legacy::LedgerService;
use proofgap::spec::{self, MAX_BALANCE as MAX};
use proofgap::tiers::{Variant, property_tier};
use serde_json::{Value, json};

const FNV_PRIME: u64 = 0x100000001B3;
const FNV_OFFSET: u64 = 0xCBF29CE484222325;

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn transfer_input(&mut self) -> (i64, i64, i64) {
        let src = (self.next() % (MAX as u64 + 1)) as i64;
        let dst = (self.next() % (MAX as u64 + 1)) as i64;
        let amount = (self.next() % (2 * MAX as u64 + 1)) as i64 - MAX;
        (src, dst, amount)
    }
}

fn checksum(outputs: impl Iterator<Item = (i64, i64, i64)>) -> u64 {
    let mut h = FNV_OFFSET;
    for (a, b, c) in outputs {
        for x in [a, b, c] {
            h = (h ^ x as u64).wrapping_mul(FNV_PRIME);
        }
    }
    h
}

fn timed<T>(repeat: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = vec![];
    let mut out = None;
    for _ in 0..repeat {
        let t = Instant::now();
        out = Some(f());
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (times[times.len() / 2], out.unwrap())
}

fn bench_exec(f: impl Fn(i64, i64, i64) -> (i64, i64, i64), xs: &[(i64, i64, i64)]) -> Value {
    let (secs, h) = timed(3, || {
        checksum(
            xs.iter()
                .map(|&(s, d, a)| f(black_box(s), black_box(d), black_box(a))),
        )
    });
    json!({"calls": xs.len(), "seconds": secs, "ns_per_call": secs / xs.len() as f64 * 1e9, "checksum": format!("{h:016x}")})
}

fn brute_force(n: u64) -> Value {
    let mut rng = SplitMix64(7);
    let t = Instant::now();
    let mut found = Value::Null;
    for _ in 0..n {
        let (s, d, a) = rng.transfer_input();
        if fee_helper_drift_high_value(black_box(s), black_box(d), black_box(a)).0 < 0 {
            found = json!({"src": s, "dst": d, "amount": a});
            break;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    json!({"samples": n, "seconds": secs, "samples_per_second": n as f64 / secs, "found": found})
}

fn main() {
    let mut rng = SplitMix64(42);
    let xs: Vec<(i64, i64, i64)> = (0..1_000_000).map(|_| rng.transfer_input()).collect();
    let legacy = LedgerService::default();
    let hv_prog = spec::spec_fee_helper_drift_high_value();
    let mut r = serde_json::Map::new();

    eprintln!("exec ...");
    r.insert("exec_compiled".into(), bench_exec(baseline, &xs));
    r.insert(
        "exec_legacy".into(),
        bench_exec(|s, d, a| legacy.as_function(s, d, a), &xs),
    );

    eprintln!("oracle ...");
    let oracle = Oracle::new(&spec::spec());
    let few = &xs[..2_000];
    let (secs, h) = timed(3, || {
        checksum(few.iter().map(|&(s, d, a)| oracle.eval(s, d, a)))
    });
    let compiled_h = checksum(few.iter().map(|&(s, d, a)| baseline(s, d, a)));
    r.insert(
        "oracle".into(),
        json!({"calls": few.len(), "seconds": secs, "us_per_call": secs / few.len() as f64 * 1e6,
               "checksum": format!("{h:016x}"), "matches_compiled": h == compiled_h}),
    );

    eprintln!("proof ...");
    let s = spec::spec();
    let (secs, res) = timed(20, || prove(&s));
    r.insert(
        "prove_spec".into(),
        json!({"promises": res.len(), "ms": secs * 1e3, "all_proved": res.iter().all(|x| x.status == Status::Proved)}),
    );
    let (secs, res) = timed(20, || prove(&hv_prog));
    let refuted: Vec<&str> = res
        .iter()
        .filter(|x| matches!(x.status, Status::Refuted(_)))
        .map(|x| x.promise.as_str())
        .collect();
    r.insert(
        "prove_high_value_drift".into(),
        json!({"ms": secs * 1e3, "refuted": refuted}),
    );
    let (secs, w) = timed(5, || boundary_witnesses(&s, 2));
    r.insert(
        "boundary_witnesses".into(),
        json!({"ms": secs * 1e3, "count": w.len()}),
    );

    eprintln!("property tier ...");
    let base_v = Variant {
        name: "baseline".into(),
        layer: "-".into(),
        story: String::new(),
        artifact: Box::new(baseline),
        source: Some(s.clone()),
        reference: s.clone(),
        generated: None,
    };
    let (secs, _) = timed(3, || property_tier(&base_v, 1_000));
    r.insert("property_1000".into(), json!({"ms": secs * 1e3}));
    let hv_v = Variant {
        name: "hv".into(),
        layer: "source".into(),
        story: String::new(),
        artifact: Box::new(fee_helper_drift_high_value),
        source: Some(hv_prog.clone()),
        reference: hv_prog.clone(),
        generated: None,
    };
    let (secs, res) = timed(1, || property_tier(&hv_v, 100_000));
    r.insert(
        "property_100k_high_value_drift".into(),
        json!({"seconds": secs, "status": res.status.key()}),
    );

    eprintln!("brute force ...");
    r.insert(
        "brute_force_high_value_drift".into(),
        brute_force(1_000_000_000),
    );

    let out = json!({"language": "rust", "rustc": env!("RUSTC_VERSION"),
                     "machine": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
                     "results": Value::Object(r.clone())});
    std::fs::write(
        "bench.json",
        serde_json::to_string_pretty(&out).unwrap() + "\n",
    )
    .unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&Value::Object(r)).unwrap()
    );
}
