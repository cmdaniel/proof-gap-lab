"""Direct evaluator for DSL expressions. Used to check promises on concrete runs."""

from __future__ import annotations

from .dsl import (
    Add,
    And,
    BoolConst,
    CeilDiv,
    Cmp,
    Const,
    Expr,
    FloorDiv,
    If,
    Implies,
    Mul,
    Not,
    Or,
    Program,
    Sub,
    Var,
)

_CMP = {
    "<": lambda a, b: a < b,
    "<=": lambda a, b: a <= b,
    "==": lambda a, b: a == b,
    ">=": lambda a, b: a >= b,
    ">": lambda a, b: a > b,
}


def evaluate(e: Expr, env: dict[str, int | bool]):
    match e:
        case Var(n): return env[n]
        case Const(v) | BoolConst(v): return v
        case Add(a, b): return evaluate(a, env) + evaluate(b, env)
        case Sub(a, b): return evaluate(a, env) - evaluate(b, env)
        case Mul(a, b): return evaluate(a, env) * evaluate(b, env)
        case FloorDiv(a, d): return evaluate(a, env) // d
        case CeilDiv(a, d): return -((-evaluate(a, env)) // d)
        case Cmp(op, a, b): return _CMP[op](evaluate(a, env), evaluate(b, env))
        case And(ps): return all(evaluate(p, env) for p in ps)
        case Or(ps): return any(evaluate(p, env) for p in ps)
        case Not(a): return not evaluate(a, env)
        case Implies(a, b): return (not evaluate(a, env)) or evaluate(b, env)
        case If(c, t, o): return evaluate(t, env) if evaluate(c, env) else evaluate(o, env)
    raise TypeError(f"unknown node {e!r}")


def broken_promises(prog: Program, inputs: dict[str, int], outputs: tuple[int, ...]) -> list[str]:
    """Return the names of promises that a concrete (inputs -> outputs) run violates."""
    env = dict(inputs)
    env.update(zip(prog.output_names, outputs))
    return [name for name, p in prog.promises.items() if not evaluate(p, env)]
