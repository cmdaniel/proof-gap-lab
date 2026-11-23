"""The "trusted kernel": translate DSL programs to Z3 and prove their promises.

A proof here is a statement about the DSL *source* under Z3's integer
semantics, for every input in the declared domain. It says nothing about the
Python that compiler.py emits, nor about hand-written code (legacy.py).
"""

from __future__ import annotations

from dataclasses import dataclass

import z3

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
    walk,
)


def to_z3(e: Expr, env: dict[str, z3.ExprRef]) -> z3.ExprRef:
    match e:
        case Var(n): return env[n]
        case Const(v): return z3.IntVal(v)
        case BoolConst(v): return z3.BoolVal(v)
        case Add(a, b): return to_z3(a, env) + to_z3(b, env)
        case Sub(a, b): return to_z3(a, env) - to_z3(b, env)
        case Mul(a, b): return to_z3(a, env) * to_z3(b, env)
        # Z3 integer division by a positive constant is floor division.
        case FloorDiv(a, d): return to_z3(a, env) / d
        case CeilDiv(a, d): return -((-to_z3(a, env)) / d)
        case Cmp(op, a, b):
            x, y = to_z3(a, env), to_z3(b, env)
            return {"<": x < y, "<=": x <= y, "==": x == y, ">=": x >= y, ">": x > y}[op]
        case And(ps): return z3.And(*[to_z3(p, env) for p in ps])
        case Or(ps): return z3.Or(*[to_z3(p, env) for p in ps])
        case Not(a): return z3.Not(to_z3(a, env))
        case Implies(a, b): return z3.Implies(to_z3(a, env), to_z3(b, env))
        case If(c, t, o): return z3.If(to_z3(c, env), to_z3(t, env), to_z3(o, env))
    raise TypeError(f"unknown node {e!r}")


@dataclass
class Model:
    """Symbolic form of a program: input vars, domain, and a full environment."""
    inputs: dict[str, z3.ArithRef]
    domain: z3.BoolRef
    env: dict[str, z3.ExprRef]
    outputs: dict[str, z3.ExprRef]


def symbolic(prog: Program) -> Model:
    inputs = {n: z3.Int(n) for n in prog.inputs}
    domain = z3.And(*[z3.And(lo <= v, v <= hi)
                      for (n, (lo, hi)), v in zip(prog.inputs.items(), inputs.values())])
    env: dict[str, z3.ExprRef] = dict(inputs)
    for name, e in prog.lets:
        env[name] = to_z3(e, env)
    outputs = {}
    for name, e in prog.outputs:
        outputs[name] = env[name] = to_z3(e, env)
    return Model(inputs, domain, env, outputs)


@dataclass
class ProofResult:
    promise: str
    status: str  # "proved" | "refuted" | "unknown"
    counterexample: dict[str, int] | None = None


def prove(prog: Program, timeout_ms: int = 10_000) -> list[ProofResult]:
    """Prove each promise for ALL inputs in the domain, or return a counterexample."""
    m = symbolic(prog)
    results = []
    for name, promise in prog.promises.items():
        s = z3.Solver()
        s.set("timeout", timeout_ms)
        s.add(m.domain, z3.Not(to_z3(promise, m.env)))
        r = s.check()
        if r == z3.unsat:
            results.append(ProofResult(name, "proved"))
        elif r == z3.sat:
            model = s.model()
            cex = {n: model.eval(v, model_completion=True).as_long() for n, v in m.inputs.items()}
            results.append(ProofResult(name, "refuted", cex))
        else:
            results.append(ProofResult(name, "unknown"))
    return results


class Oracle:
    """Concrete evaluation of the *proved* Z3 terms. The semantics the proof
    talks about, used as the reference when testing an executable."""

    def __init__(self, prog: Program):
        self.prog = prog
        self.m = symbolic(prog)

    def __call__(self, **inputs: int) -> tuple[int, ...]:
        subs = [(self.m.inputs[n], z3.IntVal(v)) for n, v in inputs.items()]
        return tuple(z3.simplify(z3.substitute(t, *subs)).as_long()
                     for t in self.m.outputs.values())


def boundary_witnesses(prog: Program, per_target: int = 2) -> list[dict[str, int]]:
    """Ask the solver for inputs that sit exactly on (and either side of) every
    decision boundary in the program, with the sibling conditions held true
    (an MC/DC-flavoured corpus). Proof tooling generating better tests."""
    m = symbolic(prog)
    exprs = [e for _, e in prog.lets] + [e for _, e in prog.outputs]
    nodes = [n for e in exprs for n in walk(e)]

    targets: list[z3.BoolRef] = []
    in_and: set[int] = set()
    for node in nodes:
        if isinstance(node, And):
            parts = [to_z3(p, m.env) for p in node.parts]
            for i, part in enumerate(node.parts):
                if isinstance(part, Cmp):
                    in_and.add(id(part))
                    diff = to_z3(part.a, m.env) - to_z3(part.b, m.env)
                    others = parts[:i] + parts[i + 1:]
                    targets += [z3.And(diff == k, *others) for k in (-1, 0, 1)]
    for node in nodes:
        if isinstance(node, Cmp) and id(node) not in in_and:
            diff = to_z3(node.a, m.env) - to_z3(node.b, m.env)
            targets += [diff == k for k in (-1, 0, 1)]
        if isinstance(node, (CeilDiv, FloorDiv)):
            num = to_z3(node.a, m.env)
            targets += [z3.And(num > 0, num % node.d == r) for r in (0, 1, node.d - 1)]

    seen, out = set(), []
    for t in targets:
        s = z3.Solver()
        s.add(m.domain, t)
        for _ in range(per_target):
            if s.check() != z3.sat:
                break
            model = s.model()
            w = {n: model.eval(v, model_completion=True).as_long() for n, v in m.inputs.items()}
            s.add(z3.Or(*[v != w[n] for n, v in m.inputs.items()]))
            key = tuple(sorted(w.items()))
            if key not in seen:
                seen.add(key)
                out.append(w)
    return out
