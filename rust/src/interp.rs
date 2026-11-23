//! Direct evaluator for DSL expressions. Used to check promises on concrete runs.

use std::collections::HashMap;

use crate::dsl::{CmpOp, Expr, Program};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    I(i128),
    B(bool),
}

impl Value {
    fn i(self) -> i128 {
        match self {
            Value::I(v) => v,
            Value::B(_) => panic!("expected int"),
        }
    }
    fn b(self) -> bool {
        match self {
            Value::B(v) => v,
            Value::I(_) => panic!("expected bool"),
        }
    }
}

/// Evaluated in i128 so promise checks cannot themselves overflow.
pub fn evaluate(e: &Expr, env: &HashMap<String, Value>) -> Value {
    use Value::*;
    match e {
        Expr::Var(n) => env[n],
        Expr::Const(v) => I(*v as i128),
        Expr::BoolConst(v) => B(*v),
        Expr::Add(a, b) => I(evaluate(a, env).i() + evaluate(b, env).i()),
        Expr::Sub(a, b) => I(evaluate(a, env).i() - evaluate(b, env).i()),
        Expr::Mul(a, b) => I(evaluate(a, env).i() * evaluate(b, env).i()),
        Expr::FloorDiv(a, d) => I(evaluate(a, env).i().div_euclid(*d as i128)),
        Expr::CeilDiv(a, d) => I(-(-evaluate(a, env).i()).div_euclid(*d as i128)),
        Expr::Cmp(op, a, b) => {
            let (x, y) = (evaluate(a, env).i(), evaluate(b, env).i());
            B(match op {
                CmpOp::Lt => x < y,
                CmpOp::Le => x <= y,
                CmpOp::Eq => x == y,
                CmpOp::Ge => x >= y,
                CmpOp::Gt => x > y,
            })
        }
        Expr::And(ps) => B(ps.iter().all(|p| evaluate(p, env).b())),
        Expr::Or(ps) => B(ps.iter().any(|p| evaluate(p, env).b())),
        Expr::Not(a) => B(!evaluate(a, env).b()),
        Expr::Implies(a, b) => B(!evaluate(a, env).b() || evaluate(b, env).b()),
        Expr::If(c, t, o) => {
            if evaluate(c, env).b() {
                evaluate(t, env)
            } else {
                evaluate(o, env)
            }
        }
    }
}

/// Names of the promises a concrete (inputs -> outputs) run violates.
pub fn broken_promises(
    prog: &Program,
    inputs: &[(&str, i64)],
    outputs: (i64, i64, i64),
) -> Vec<String> {
    let mut env: HashMap<String, Value> = inputs
        .iter()
        .map(|(n, v)| (n.to_string(), Value::I(*v as i128)))
        .collect();
    let outs = [outputs.0, outputs.1, outputs.2];
    for (name, v) in prog.output_names().into_iter().zip(outs) {
        env.insert(name.to_string(), Value::I(v as i128));
    }
    prog.promises
        .iter()
        .filter(|(_, p)| !evaluate(p, &env).b())
        .map(|(n, _)| n.clone())
        .collect()
}
