//! proof-gap lab, Rust edition. See the repository README.

pub mod catalog;
pub mod checker;
pub mod compiler;
pub mod dsl;
pub mod interp;
pub mod legacy;
pub mod spec;
pub mod tiers;

/// Functions emitted by the unverified compiler in `build.rs`, compiled by rustc.
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/generated.rs"));
}

use legacy::LedgerService;
use tiers::Variant;

/// Every variant of the experiment, in the same order as the Python lab.
pub fn variants() -> Vec<Variant> {
    let mut out: Vec<Variant> = catalog::COMPILED
        .iter()
        .map(|c| {
            let (_, f, src) = generated::COMPILED_FNS
                .iter()
                .find(|(n, _, _)| *n == c.name)
                .expect("compiled");
            let f = *f;
            Variant {
                name: c.name.into(),
                layer: c.layer.into(),
                story: c.story.into(),
                artifact: Box::new(f),
                source: Some((c.source)()),
                reference: (c.source)(),
                generated: Some(src.to_string()),
            }
        })
        .collect();
    for (name, story, strict) in [
        ("legacy service", "Hand-written Rust, correct.", false),
        (
            "legacy: strict check",
            "Hand-written Rust with '<' instead of '<=' in the affordability check: \
             you can no longer spend your whole balance.",
            true,
        ),
    ] {
        let svc = LedgerService {
            strict_affordability: strict,
        };
        out.push(Variant {
            name: name.into(),
            layer: "legacy".into(),
            story: story.into(),
            artifact: Box::new(move |s, d, a| svc.as_function(s, d, a)),
            source: None,
            reference: spec::spec(),
            generated: None,
        });
    }
    out
}
