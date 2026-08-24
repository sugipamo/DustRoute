from __future__ import annotations
from dataclasses import dataclass
from enum import Enum, auto
from typing import Mapping
from .model import GateKind

class Direction(Enum): IN=auto(); OUT=auto()
@dataclass(frozen=True)
class Gate: id:int; kind:GateKind; input_count:int; output_count:int=1
@dataclass(frozen=True)
class Pin: gate:int; direction:Direction; index:int
@dataclass(frozen=True)
class Net: id:int; source:Pin; sinks:tuple[Pin,...]
@dataclass(frozen=True)
class Circuit: gates:tuple[Gate,...]; nets:tuple[Net,...]

class Expr: pass
@dataclass(frozen=True)
class Var(Expr): name:str
@dataclass(frozen=True)
class Const(Expr): value:bool
@dataclass(frozen=True)
class Not(Expr): value:Expr
@dataclass(frozen=True)
class And(Expr):
    values:tuple[Expr,...]
    def __init__(self,*xs):object.__setattr__(self,'values',tuple(xs))
@dataclass(frozen=True)
class Or(Expr):
    values:tuple[Expr,...]
    def __init__(self,*xs):object.__setattr__(self,'values',tuple(xs))
@dataclass(frozen=True)
class Xor(Expr):
    values:tuple[Expr,...]
    def __init__(self,*xs):object.__setattr__(self,'values',tuple(xs))

@dataclass(frozen=True)
class Nand(Expr):
    values:tuple[Expr,...]
    def __init__(self,*xs):object.__setattr__(self,'values',tuple(xs))

def evaluate(e,env:Mapping[str,bool]):
    if isinstance(e,Var):return env[e.name]
    if isinstance(e,Const):return e.value
    if isinstance(e,Not):return not evaluate(e.value,env)
    if isinstance(e,And):return all(evaluate(x,env) for x in e.values)
    if isinstance(e,Or):return any(evaluate(x,env) for x in e.values)
    if isinstance(e,Xor):return sum(evaluate(x,env) for x in e.values)%2==1
    if isinstance(e,Nand):return not all(evaluate(x,env) for x in e.values)
    raise TypeError(type(e))

def expr_size(e):
    if isinstance(e,(Var,Const)):return 1
    if isinstance(e,Not):return 1+expr_size(e.value)
    return 1+sum(expr_size(x) for x in e.values)

def rewrites_once(e):
    out=set()
    # bidirectional local identities used by current optimizer
    if isinstance(e,Not) and isinstance(e.value,Not):out.add(e.value.value)
    else:out.add(Not(Not(e)))
    if isinstance(e,Not) and isinstance(e.value,And) and len(e.value.values)==2:
        a,b=e.value.values
        out.add(Or(Not(a),Not(b)))
        out.add(Nand(a,b))
    if isinstance(e,Nand) and len(e.values)==2:
        a,b=e.values
        out.add(Not(And(a,b)))
    if isinstance(e,Not) and isinstance(e.value,Or) and len(e.value.values)==2:
        a,b=e.value.values;out.add(And(Not(a),Not(b)))
    if isinstance(e,Xor) and len(e.values)==2:
        a,b=e.values;out.add(Or(And(a,Not(b)),And(Not(a),b)))
    # recurse into children
    if isinstance(e,Not):
        for x in rewrites_once(e.value):out.add(Not(x))
    elif isinstance(e,(And,Or,Xor,Nand)):
        ctor=type(e)
        for i,v in enumerate(e.values):
            for x in rewrites_once(v):
                vals=list(e.values);vals[i]=x;out.add(ctor(*vals))
    out.discard(e);return tuple(out)

def search_equivalents(start,max_steps=3,max_states=256):
    seen={start};front=[start];ordered=[start]
    for _ in range(max_steps):
        nf=[]
        for e in front:
            for c in rewrites_once(e):
                if c in seen:continue
                seen.add(c);ordered.append(c);nf.append(c)
                if len(ordered)>=max_states:return tuple(ordered)
        if not nf:break
        front=nf
    return tuple(ordered)

def best_by_expr_size(start,**kwargs):return min(search_equivalents(start,**kwargs),key=lambda e:(expr_size(e),repr(e)))
