//! The "trusted kernel": translate DSL programs to Z3 and prove their promises.
//! A port of `python/src/proofgap/checker.py` that builds the same Z3 terms
//! (same operators, same argument order) so both languages ask Z3 the same
//! questions.
//!
//! Rust-only addition: `overflow_obligations` proves that every integer
//! subterm fits in `i64` across the whole domain. The proof's integers are
//! mathematical; the compiled code's are 64-bit. Python never needs this.

use std::collections::{HashMap, HashSet};

use z3::ast::{Ast, Bool, Int};
use z3::{Model as Z3Model, Params, SatResult, Solver};

use crate::dsl::{CmpOp, Expr, Program, walk};

#[derive(Clone, Debug)]
pub enum Val {
    I(Int),
    B(Bool),
}

impl Val {
    pub fn i(&self) -> &Int {
        match self {
            Val::I(v) => v,
            Val::B(_) => panic!("expected int term"),
        }
    }
    pub fn b(&self) -> &Bool {
        match self {
            Val::B(v) => v,
            Val::I(_) => panic!("expected bool term"),
        }
    }
}

fn add2(a: Int, b: Int) -> Int {
    Int::add(&[a, b])
}

pub fn to_z3(e: &Expr, env: &HashMap<String, Val>) -> Val {
    let i = |x: &Expr| to_z3(x, env).i().clone();
    let b = |x: &Expr| to_z3(x, env).b().clone();
    match e {
        Expr::Var(n) => env[n].clone(),
        Expr::Const(v) => Val::I(Int::from_i64(*v)),
        Expr::BoolConst(v) => Val::B(Bool::from_bool(*v)),
        Expr::Add(x, y) => Val::I(add2(i(x), i(y))),
        Expr::Sub(x, y) => Val::I(Int::sub(&[i(x), i(y)])),
        Expr::Mul(x, y) => Val::I(Int::mul(&[i(x), i(y)])),
        // Z3 integer division by a positive constant is floor division.
        Expr::FloorDiv(x, d) => Val::I(i(x).div(Int::from_i64(*d))),
        Expr::CeilDiv(x, d) => Val::I(i(x).unary_minus().div(Int::from_i64(*d)).unary_minus()),
        Expr::Cmp(op, x, y) => {
            let (l, r) = (i(x), i(y));
            Val::B(match op {
                CmpOp::Lt => l.lt(r),
                CmpOp::Le => l.le(r),
                CmpOp::Eq => l.eq(r),
                CmpOp::Ge => l.ge(r),
                CmpOp::Gt => l.gt(r),
            })
        }
        Expr::And(ps) => Val::B(Bool::and(&ps.iter().map(&b).collect::<Vec<_>>())),
        Expr::Or(ps) => Val::B(Bool::or(&ps.iter().map(&b).collect::<Vec<_>>())),
        Expr::Not(x) => Val::B(b(x).not()),
        Expr::Implies(x, y) => Val::B(b(x).implies(b(y))),
        Expr::If(c, t, o) => match (to_z3(t, env), to_z3(o, env)) {
            (Val::I(t), Val::I(o)) => Val::I(b(c).ite(&t, &o)),
            (Val::B(t), Val::B(o)) => Val::B(b(c).ite(&t, &o)),
            _ => panic!("ill-typed if"),
        },
    }
}

/// Symbolic form of a program: input vars, domain, and a full environment.
pub struct Symbolic {
    pub inputs: Vec<(String, Int)>,
    pub domain: Bool,
    pub env: HashMap<String, Val>,
    pub outputs: Vec<Int>,
}

pub fn symbolic(prog: &Program) -> Symbolic {
    let inputs: Vec<(String, Int)> = prog
        .inputs
        .iter()
        .map(|(n, _)| (n.clone(), Int::new_const(n.as_str())))
        .collect();
    // Python writes `lo <= v`, which z3py turns into `v >= lo`; match it.
    let ranges: Vec<Bool> = prog
        .inputs
        .iter()
        .zip(&inputs)
        .map(|((_, (lo, hi)), (_, v))| {
            Bool::and(&[v.ge(Int::from_i64(*lo)), v.le(Int::from_i64(*hi))])
        })
        .collect();
    let domain = Bool::and(&ranges);
    let mut env: HashMap<String, Val> = inputs
        .iter()
        .map(|(n, v)| (n.clone(), Val::I(v.clone())))
        .collect();
    for (name, e) in &prog.lets {
        let t = to_z3(e, &env);
        env.insert(name.clone(), t);
    }
    let mut outputs = vec![];
    for (name, e) in &prog.outputs {
        let t = to_z3(e, &env);
        outputs.push(t.i().clone());
        env.insert(name.clone(), t);
    }
    Symbolic {
        inputs,
        domain,
        env,
        outputs,
    }
}

fn read_inputs(model: &Z3Model, inputs: &[(String, Int)]) -> Vec<(String, i64)> {
    inputs
        .iter()
        .map(|(n, v)| (n.clone(), model.eval(v, true).unwrap().as_i64().unwrap()))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Proved,
    Refuted(Vec<(String, i64)>),
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ProofResult {
    pub promise: String,
    pub status: Status,
}

fn solver_with_timeout(ms: u32) -> Solver {
    let s = Solver::new();
    let mut p = Params::new();
    p.set_u32("timeout", ms);
    s.set_params(&p).expect("set timeout");
    s
}

/// Prove each promise for ALL inputs in the domain, or return a counterexample.
pub fn prove(prog: &Program) -> Vec<ProofResult> {
    let m = symbolic(prog);
    prog.promises
        .iter()
        .map(|(name, promise)| {
            let s = solver_with_timeout(10_000);
            s.assert(&m.domain);
            s.assert(to_z3(promise, &m.env).b().not());
            let status = match s.check() {
                SatResult::Unsat => Status::Proved,
                SatResult::Sat => Status::Refuted(read_inputs(&s.get_model().unwrap(), &m.inputs)),
                SatResult::Unknown => Status::Unknown,
            };
            ProofResult {
                promise: name.clone(),
                status,
            }
        })
        .collect()
}

/// Rust-only: prove every integer subterm of the program stays inside i64 for
/// every input in the domain. Returns (subterms checked, subterms that can overflow).
pub fn overflow_obligations(prog: &Program) -> (usize, Vec<String>) {
    let m = symbolic(prog);
    let (lo, hi) = (Int::from_i64(i64::MIN), Int::from_i64(i64::MAX));
    let mut checked = 0;
    let mut failing = vec![];
    for (_, e) in prog.lets.iter().chain(prog.outputs.iter()) {
        for node in walk(e) {
            if let Val::I(t) = to_z3(node, &m.env) {
                checked += 1;
                let s = solver_with_timeout(10_000);
                s.assert(&m.domain);
                s.assert(Bool::or(&[t.lt(lo.clone()), t.gt(hi.clone())]));
                if s.check() != SatResult::Unsat {
                    failing.push(format!("{node:?}"));
                }
            }
        }
    }
    (checked, failing)
}

/// Concrete evaluation of the *proved* Z3 terms: the reference when testing
/// an executable. Same substitute + simplify approach as the Python Oracle.
pub struct Oracle {
    m: Symbolic,
}

impl Oracle {
    pub fn new(prog: &Program) -> Self {
        Oracle { m: symbolic(prog) }
    }

    pub fn eval(&self, src: i64, dst: i64, amount: i64) -> (i64, i64, i64) {
        let vals = [
            Int::from_i64(src),
            Int::from_i64(dst),
            Int::from_i64(amount),
        ];
        let subs: Vec<(&Int, &Int)> = self
            .m
            .inputs
            .iter()
            .map(|(_, v)| v)
            .zip(vals.iter())
            .collect();
        let o: Vec<i64> = self
            .m
            .outputs
            .iter()
            .map(|t| t.substitute(&subs).simplify().as_i64().expect("concrete"))
            .collect();
        (o[0], o[1], o[2])
    }
}

/// Solver-chosen inputs on and either side of every decision boundary, with
/// sibling conditions held true (MC/DC-flavoured). Same targets, same order
/// as the Python version.
pub fn boundary_witnesses(prog: &Program, per_target: usize) -> Vec<Vec<(String, i64)>> {
    let m = symbolic(prog);
    let exprs: Vec<&Expr> = prog
        .lets
        .iter()
        .chain(prog.outputs.iter())
        .map(|(_, e)| e)
        .collect();
    let nodes: Vec<&Expr> = exprs.into_iter().flat_map(walk).collect();
    let diff = |a: &Expr, b: &Expr| {
        Int::sub(&[to_z3(a, &m.env).i().clone(), to_z3(b, &m.env).i().clone()])
    };

    let mut targets: Vec<Bool> = vec![];
    let mut in_and: HashSet<*const Expr> = HashSet::new();
    for node in &nodes {
        if let Expr::And(parts) = node {
            let zs: Vec<Bool> = parts.iter().map(|p| to_z3(p, &m.env).b().clone()).collect();
            for (i, part) in parts.iter().enumerate() {
                if let Expr::Cmp(_, a, b) = part {
                    in_and.insert(part as *const Expr);
                    let d = diff(a, b);
                    let others: Vec<Bool> = zs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, z)| z.clone())
                        .collect();
                    for k in [-1, 0, 1] {
                        let mut conj = vec![d.eq(Int::from_i64(k))];
                        conj.extend(others.iter().cloned());
                        targets.push(Bool::and(&conj));
                    }
                }
            }
        }
    }
    for node in &nodes {
        if let Expr::Cmp(_, a, b) = node
            && !in_and.contains(&(*node as *const Expr))
        {
            let d = diff(a, b);
            targets.extend([-1, 0, 1].map(|k| d.eq(Int::from_i64(k))));
        }
        if let Expr::CeilDiv(a, d) | Expr::FloorDiv(a, d) = node {
            let num = to_z3(a, &m.env).i().clone();
            for r in [0, 1, d - 1] {
                targets.push(Bool::and(&[
                    num.gt(Int::from_i64(0)),
                    num.modulo(Int::from_i64(*d)).eq(Int::from_i64(r)),
                ]));
            }
        }
    }

    let mut seen = HashSet::new();
    let mut out = vec![];
    for t in targets {
        let s = Solver::new();
        s.assert(&m.domain);
        s.assert(&t);
        for _ in 0..per_target {
            if s.check() != SatResult::Sat {
                break;
            }
            let w = read_inputs(&s.get_model().unwrap(), &m.inputs);
            let block: Vec<Bool> = m
                .inputs
                .iter()
                .zip(&w)
                .map(|((_, v), (_, x))| Int::distinct(&[v.clone(), Int::from_i64(*x)]))
                .collect();
            s.assert(Bool::or(&block));
            let mut key = w.clone();
            key.sort();
            if seen.insert(key) {
                out.push(w);
            }
        }
    }
    out
}
