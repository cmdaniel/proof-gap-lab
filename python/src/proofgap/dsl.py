"""A tiny integer expression language: the "proof language" of this lab.

Programs written here can be (a) proved with an SMT solver (checker.py) and
(b) compiled to executable Python (compiler.py). Those are two different
translations of the same source, which is exactly where the gap lives.

Semantics: unbounded mathematical integers and booleans, no side effects.
"""

from __future__ import annotations

from dataclasses import dataclass, field


class Expr:
    """Base class. Operator overloads keep spec files readable."""

    def __add__(self, o): return Add(self, lift(o))
    def __radd__(self, o): return Add(lift(o), self)
    def __sub__(self, o): return Sub(self, lift(o))
    def __rsub__(self, o): return Sub(lift(o), self)
    def __mul__(self, o): return Mul(self, lift(o))
    def __rmul__(self, o): return Mul(lift(o), self)


def lift(x) -> Expr:
    if isinstance(x, Expr):
        return x
    if isinstance(x, bool):
        return BoolConst(x)
    if isinstance(x, int):
        return Const(x)
    raise TypeError(f"cannot lift {x!r} into the DSL")


@dataclass(frozen=True)
class Var(Expr):
    name: str


@dataclass(frozen=True)
class Const(Expr):
    value: int


@dataclass(frozen=True)
class BoolConst(Expr):
    value: bool


@dataclass(frozen=True)
class Add(Expr):
    a: Expr
    b: Expr


@dataclass(frozen=True)
class Sub(Expr):
    a: Expr
    b: Expr


@dataclass(frozen=True)
class Mul(Expr):
    a: Expr
    b: Expr


@dataclass(frozen=True)
class FloorDiv(Expr):
    """Floor division by a positive integer constant."""
    a: Expr
    d: int


@dataclass(frozen=True)
class CeilDiv(Expr):
    """Ceiling division by a positive integer constant."""
    a: Expr
    d: int


@dataclass(frozen=True)
class Cmp(Expr):
    op: str  # one of "<", "<=", "==", ">=", ">"
    a: Expr
    b: Expr


@dataclass(frozen=True)
class And(Expr):
    parts: tuple[Expr, ...]


@dataclass(frozen=True)
class Or(Expr):
    parts: tuple[Expr, ...]


@dataclass(frozen=True)
class Not(Expr):
    a: Expr


@dataclass(frozen=True)
class Implies(Expr):
    a: Expr
    b: Expr


@dataclass(frozen=True)
class If(Expr):
    cond: Expr
    then: Expr
    other: Expr


# Small constructors so specs read like maths.
def lt(a, b): return Cmp("<", lift(a), lift(b))
def le(a, b): return Cmp("<=", lift(a), lift(b))
def eq(a, b): return Cmp("==", lift(a), lift(b))
def ge(a, b): return Cmp(">=", lift(a), lift(b))
def gt(a, b): return Cmp(">", lift(a), lift(b))
def all_of(*xs): return And(tuple(lift(x) for x in xs))
def any_of(*xs): return Or(tuple(lift(x) for x in xs))
def implies(a, b): return Implies(lift(a), lift(b))
def ite(c, t, e): return If(lift(c), lift(t), lift(e))


@dataclass(frozen=True)
class Program:
    """A straight-line function: inputs -> lets -> outputs, plus promises.

    inputs:   name -> (lo, hi) inclusive domain
    lets:     ordered (name, expr); later lets may refer to earlier ones
    outputs:  ordered (name, expr)
    promises: name -> boolean expr over inputs and outputs (the "core promises")
    """

    name: str
    inputs: dict[str, tuple[int, int]]
    lets: tuple[tuple[str, Expr], ...]
    outputs: tuple[tuple[str, Expr], ...]
    promises: dict[str, Expr] = field(default_factory=dict)

    @property
    def input_names(self) -> list[str]:
        return list(self.inputs)

    @property
    def output_names(self) -> list[str]:
        return [n for n, _ in self.outputs]


def walk(e: Expr):
    """Yield every node in an expression tree (pre-order)."""
    yield e
    match e:
        case Add(a, b) | Sub(a, b) | Mul(a, b) | Cmp(_, a, b) | Implies(a, b):
            yield from walk(a)
            yield from walk(b)
        case FloorDiv(a, _) | CeilDiv(a, _) | Not(a):
            yield from walk(a)
        case And(parts) | Or(parts):
            for p in parts:
                yield from walk(p)
        case If(c, t, o):
            yield from walk(c)
            yield from walk(t)
            yield from walk(o)
