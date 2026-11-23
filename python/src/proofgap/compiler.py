"""The "untrusted compiler": DSL source -> Python source -> callable.

Nothing here is proved. It is an ordinary, plausible code generator that
emits minimal parentheses using a precedence table. The two `bug` flags are
one-character slips of the kind real compilers ship:

  assoc_bug: treats `-` as associative when deciding whether the right
             operand needs parentheses, so  a - (b + c)  is emitted as
             a - b + c.
  ceil_bug:  a misplaced parenthesis in the ceiling-division template,
             -(-a // d)  becomes  -(-a) // d, which is floor division.

The checker proves the DSL source; it never sees this file's output.
"""

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

# Python operator precedence, low to high.
COND, OR, AND, NOT, CMP, ADD, MUL, UNARY, ATOM = range(1, 10)


class Emitter:
    def __init__(self, assoc_bug: bool = False, ceil_bug: bool = False):
        self.assoc_bug = assoc_bug
        self.ceil_bug = ceil_bug

    def wrap(self, e: Expr, min_prec: int) -> str:
        """Emit e, parenthesising it if it binds looser than min_prec."""
        src, prec = self.emit(e)
        return f"({src})" if prec < min_prec else src

    def binop(self, a: Expr, op: str, b: Expr, prec: int) -> tuple[str, int]:
        left = self.wrap(a, prec)
        # Right operand of a left-associative operator must be parenthesised
        # when it binds *as loosely or looser* (<=). The bug uses < instead.
        right = self.wrap(b, prec if self.assoc_bug else prec + 1)
        return f"{left} {op} {right}", prec

    def emit(self, e: Expr) -> tuple[str, int]:
        match e:
            case Var(n): return n, ATOM
            case Const(v): return str(v), (UNARY if v < 0 else ATOM)
            case BoolConst(v): return str(v), ATOM
            case Add(a, b): return self.binop(a, "+", b, ADD)
            case Sub(a, b): return self.binop(a, "-", b, ADD)
            case Mul(a, b): return self.binop(a, "*", b, MUL)
            case FloorDiv(a, d): return f"{self.wrap(a, MUL)} // {d}", MUL
            case CeilDiv(a, d):
                inner = self.wrap(a, UNARY)
                if self.ceil_bug:
                    return f"-(-{inner}) // {d}", MUL
                return f"-(-{inner} // {d})", UNARY
            case Cmp(op, a, b): return f"{self.wrap(a, ADD)} {op} {self.wrap(b, ADD)}", CMP
            case And(ps): return " and ".join(self.wrap(p, AND) for p in ps), AND
            case Or(ps): return " or ".join(self.wrap(p, OR) for p in ps), OR
            case Not(a): return f"not {self.wrap(a, NOT)}", NOT
            case Implies(a, b): return self.emit(Or((Not(a), b)))
            case If(c, t, o):
                return (f"{self.wrap(t, COND + 1)} if {self.wrap(c, COND + 1)} "
                        f"else {self.wrap(o, COND)}"), COND
        raise TypeError(f"unknown node {e!r}")


def to_source(prog: Program, **bugs: bool) -> str:
    em = Emitter(**bugs)
    lines = [f"def {prog.name}({', '.join(prog.input_names)}):"]
    for name, e in [*prog.lets, *prog.outputs]:
        lines.append(f"    {name} = {em.emit(e)[0]}")
    lines.append(f"    return ({', '.join(prog.output_names)},)")
    return "\n".join(lines) + "\n"


def compile_program(prog: Program, **bugs: bool):
    """Return (callable, generated_source)."""
    src = to_source(prog, **bugs)
    ns: dict = {}
    exec(compile(src, f"<compiled {prog.name}>", "exec"), ns)  # noqa: S102 - this IS the compiler
    return ns[prog.name], src
