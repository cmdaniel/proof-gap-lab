//! A tiny integer expression language: the "proof language" of this lab.
//!
//! Programs written here can be (a) proved with an SMT solver (`checker.rs`)
//! and (b) compiled to executable Rust (`compiler.rs`, run from `build.rs`).
//! Those are two different translations of the same source, which is exactly
//! where the gap lives.
//!
//! Semantics: mathematical integers and booleans, no side effects. This file
//! is shared with `build.rs`, so it must not depend on anything outside std.

use std::ops::{Add, Mul, Sub};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Var(String),
    Const(i64),
    BoolConst(bool),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    /// Floor division by a positive integer constant.
    FloorDiv(Box<Expr>, i64),
    /// Ceiling division by a positive integer constant.
    CeilDiv(Box<Expr>, i64),
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
    Implies(Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
        }
    }
}

impl From<i64> for Expr {
    fn from(v: i64) -> Self {
        Expr::Const(v)
    }
}

impl From<bool> for Expr {
    fn from(v: bool) -> Self {
        Expr::BoolConst(v)
    }
}

impl From<&Expr> for Expr {
    fn from(e: &Expr) -> Self {
        e.clone()
    }
}

macro_rules! arith {
    ($tr:ident, $f:ident, $variant:ident) => {
        impl<T: Into<Expr>> $tr<T> for Expr {
            type Output = Expr;
            fn $f(self, o: T) -> Expr {
                Expr::$variant(Box::new(self), Box::new(o.into()))
            }
        }
        impl<T: Into<Expr>> $tr<T> for &Expr {
            type Output = Expr;
            fn $f(self, o: T) -> Expr {
                Expr::$variant(Box::new(self.clone()), Box::new(o.into()))
            }
        }
    };
}
arith!(Add, add, Add);
arith!(Sub, sub, Sub);
arith!(Mul, mul, Mul);

// Small constructors so specs read like maths.
pub fn var(name: &str) -> Expr {
    Expr::Var(name.to_string())
}
fn cmp(op: CmpOp, a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    Expr::Cmp(op, Box::new(a.into()), Box::new(b.into()))
}
pub fn lt(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    cmp(CmpOp::Lt, a, b)
}
pub fn le(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    cmp(CmpOp::Le, a, b)
}
pub fn eq(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    cmp(CmpOp::Eq, a, b)
}
pub fn ge(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    cmp(CmpOp::Ge, a, b)
}
#[allow(dead_code)]
pub fn gt(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    cmp(CmpOp::Gt, a, b)
}
pub fn all_of(parts: Vec<Expr>) -> Expr {
    Expr::And(parts)
}
pub fn implies(a: impl Into<Expr>, b: impl Into<Expr>) -> Expr {
    Expr::Implies(Box::new(a.into()), Box::new(b.into()))
}
pub fn ite(c: impl Into<Expr>, t: impl Into<Expr>, e: impl Into<Expr>) -> Expr {
    Expr::If(Box::new(c.into()), Box::new(t.into()), Box::new(e.into()))
}
pub fn floor_div(a: impl Into<Expr>, d: i64) -> Expr {
    Expr::FloorDiv(Box::new(a.into()), d)
}
pub fn ceil_div(a: impl Into<Expr>, d: i64) -> Expr {
    Expr::CeilDiv(Box::new(a.into()), d)
}

/// A straight-line function: inputs -> lets -> outputs, plus promises.
#[derive(Clone, Debug)]
pub struct Program {
    pub name: String,
    /// name -> inclusive domain
    pub inputs: Vec<(String, (i64, i64))>,
    /// ordered; later lets may refer to earlier ones
    pub lets: Vec<(String, Expr)>,
    pub outputs: Vec<(String, Expr)>,
    /// boolean expressions over inputs and outputs: the "core promises"
    pub promises: Vec<(String, Expr)>,
}

impl Program {
    pub fn input_names(&self) -> Vec<&str> {
        self.inputs.iter().map(|(n, _)| n.as_str()).collect()
    }
    pub fn output_names(&self) -> Vec<&str> {
        self.outputs.iter().map(|(n, _)| n.as_str()).collect()
    }
}

/// Every node in an expression tree, pre-order (same order as Python's `walk`).
#[allow(dead_code)]
pub fn walk(e: &Expr) -> Vec<&Expr> {
    let mut out = vec![e];
    match e {
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Cmp(_, a, b)
        | Expr::Implies(a, b) => {
            out.extend(walk(a));
            out.extend(walk(b));
        }
        Expr::FloorDiv(a, _) | Expr::CeilDiv(a, _) | Expr::Not(a) => out.extend(walk(a)),
        Expr::And(ps) | Expr::Or(ps) => ps.iter().for_each(|p| out.extend(walk(p))),
        Expr::If(c, t, o) => {
            out.extend(walk(c));
            out.extend(walk(t));
            out.extend(walk(o));
        }
        Expr::Var(_) | Expr::Const(_) | Expr::BoolConst(_) => {}
    }
    out
}
