//! The "untrusted compiler": DSL source -> Rust source. `build.rs` runs it,
//! and rustc turns the output into native code linked into this crate.
//!
//! Nothing here is proved. It is an ordinary code generator that emits
//! minimal parentheses using Rust's precedence table. The two bug flags
//! mirror the Python lab's:
//!
//!   assoc_bug: treats `-` as associative when deciding whether the right
//!              operand needs parentheses, so  a - (b + c)  is emitted as
//!              a - b + c.
//!   ceil_bug:  a misplaced parenthesis in the ceiling-division template,
//!              -(-a).div_euclid(d)  becomes  (-(-a)).div_euclid(d), which is
//!              floor division.
//!
//! Rust-specific trap avoided here: Rust's `/` on integers truncates toward
//! zero, while the DSL (like Python's `//` and Z3's `div`) floors. The
//! emitter uses `div_euclid`, which floors for a positive divisor.

use crate::dsl::{Expr, Program};

// Rust operator precedence, low to high (only the levels we emit).
const COND: u8 = 1; // if/else expression (always parenthesised when nested)
const OR: u8 = 2;
const AND: u8 = 3;
const CMP: u8 = 4;
const ADD: u8 = 5;
const MUL: u8 = 6;
const UNARY: u8 = 7;
const POSTFIX: u8 = 8; // method calls
const ATOM: u8 = 9;

#[derive(Clone, Copy, Default, Debug)]
pub struct Bugs {
    pub assoc_bug: bool,
    pub ceil_bug: bool,
}

struct Emitter {
    bugs: Bugs,
}

impl Emitter {
    fn wrap(&self, e: &Expr, min_prec: u8) -> String {
        let (src, prec) = self.emit(e);
        if prec < min_prec {
            format!("({src})")
        } else {
            src
        }
    }

    fn binop(&self, a: &Expr, op: &str, b: &Expr, prec: u8) -> (String, u8) {
        let left = self.wrap(a, prec);
        // Right operand of a left-associative operator must be parenthesised
        // when it binds *as loosely or looser*. The bug uses < instead of <=.
        let right = self.wrap(b, if self.bugs.assoc_bug { prec } else { prec + 1 });
        (format!("{left} {op} {right}"), prec)
    }

    fn emit(&self, e: &Expr) -> (String, u8) {
        match e {
            Expr::Var(n) => (n.clone(), ATOM),
            Expr::Const(v) => (format!("{v}"), if *v < 0 { UNARY } else { ATOM }),
            Expr::BoolConst(v) => (format!("{v}"), ATOM),
            Expr::Add(a, b) => self.binop(a, "+", b, ADD),
            Expr::Sub(a, b) => self.binop(a, "-", b, ADD),
            Expr::Mul(a, b) => self.binop(a, "*", b, MUL),
            Expr::FloorDiv(a, d) => (format!("{}.div_euclid({d})", self.wrap(a, ATOM)), POSTFIX),
            Expr::CeilDiv(a, d) => {
                let inner = self.wrap(a, UNARY);
                if self.bugs.ceil_bug {
                    (format!("(-(-{inner})).div_euclid({d})"), POSTFIX)
                } else {
                    (format!("-(-{inner}).div_euclid({d})"), UNARY)
                }
            }
            // Rust comparisons are non-associative: operands must bind tighter.
            Expr::Cmp(op, a, b) => (
                format!(
                    "{} {} {}",
                    self.wrap(a, ADD),
                    op.symbol(),
                    self.wrap(b, ADD)
                ),
                CMP,
            ),
            Expr::And(ps) => (
                ps.iter()
                    .map(|p| self.wrap(p, AND))
                    .collect::<Vec<_>>()
                    .join(" && "),
                AND,
            ),
            Expr::Or(ps) => (
                ps.iter()
                    .map(|p| self.wrap(p, OR))
                    .collect::<Vec<_>>()
                    .join(" || "),
                OR,
            ),
            Expr::Not(a) => (format!("!{}", self.wrap(a, UNARY)), UNARY),
            Expr::Implies(a, b) => self.emit(&Expr::Or(vec![Expr::Not(a.clone()), (**b).clone()])),
            Expr::If(c, t, o) => (
                format!(
                    "if {} {{ {} }} else {{ {} }}",
                    self.emit(c).0,
                    self.emit(t).0,
                    self.emit(o).0
                ),
                COND,
            ),
        }
    }
}

/// Emit a Rust function `fn <fn_name>(src: i64, ...) -> (i64, i64, i64)`.
pub fn to_source(prog: &Program, fn_name: &str, bugs: Bugs) -> String {
    let em = Emitter { bugs };
    let params: Vec<String> = prog
        .input_names()
        .iter()
        .map(|n| format!("{n}: i64"))
        .collect();
    let mut out = format!(
        "pub fn {fn_name}({}) -> ({}) {{\n",
        params.join(", "),
        vec!["i64"; prog.outputs.len()].join(", ")
    );
    for (name, e) in prog.lets.iter().chain(prog.outputs.iter()) {
        out.push_str(&format!("    let {name} = {};\n", em.emit(e).0));
    }
    out.push_str(&format!("    ({})\n}}\n", prog.output_names().join(", ")));
    out
}
