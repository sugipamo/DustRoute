from __future__ import annotations

from .logic_dag import DAGBuilder, LogicDAG
from .model import GateKind


def full_adder_dag() -> LogicDAG:
    """1-bit full adder: A + B + Cin -> SUM, CARRY."""
    b=DAGBuilder()
    a=b.input("a")
    x=b.input("b")
    cin=b.input("cin")

    ab=b.op(GateKind.XOR,a,x,name="ab_xor")
    sum_=b.op(GateKind.XOR,ab,cin,name="sum_xor")

    carry_ab=b.op(GateKind.AND,a,x,name="carry_ab")
    carry_cin=b.op(GateKind.AND,cin,ab,name="carry_cin")
    carry=b.op(GateKind.OR,carry_ab,carry_cin,name="carry_or")

    return b.finish((("sum",sum_),("carry",carry)))


def mux2_dag() -> LogicDAG:
    """2:1 multiplexer: OUT = (!S & A) | (S & B)."""
    b=DAGBuilder()
    a=b.input("a")
    x=b.input("b")
    s=b.input("s")

    ns=b.op(GateKind.NOT,s,name="not_s")
    left=b.op(GateKind.AND,ns,a,name="select_a")
    right=b.op(GateKind.AND,s,x,name="select_b")
    out=b.op(GateKind.OR,left,right,name="mux_out")
    return b.finish((("out",out),))


def ripple_adder_2bit_dag() -> LogicDAG:
    """
    Unsigned two-bit ripple adder:
        A = a1 a0
        B = b1 b0
        result = carry s1 s0
    """
    b=DAGBuilder()
    a0=b.input("a0")
    a1=b.input("a1")
    b0=b.input("b0")
    b1=b.input("b1")

    # bit 0 half-adder
    s0=b.op(GateKind.XOR,a0,b0,name="sum0")
    c0=b.op(GateKind.AND,a0,b0,name="carry0")

    # bit 1 full-adder with c0
    ab1=b.op(GateKind.XOR,a1,b1,name="ab1_xor")
    s1=b.op(GateKind.XOR,ab1,c0,name="sum1")
    c_ab=b.op(GateKind.AND,a1,b1,name="carry1_ab")
    c_c0=b.op(GateKind.AND,c0,ab1,name="carry1_c0")
    carry=b.op(GateKind.OR,c_ab,c_c0,name="carry1")

    return b.finish((("s0",s0),("s1",s1),("carry",carry)))


def half_subtractor_dag() -> LogicDAG:
    """Half subtractor: A - B -> DIFF, BORROW."""
    b=DAGBuilder()
    a=b.input("a")
    x=b.input("b")
    diff=b.op(GateKind.XOR,a,x,name="diff")
    na=b.op(GateKind.NOT,a,name="not_a")
    borrow=b.op(GateKind.AND,na,x,name="borrow")
    return b.finish((("diff",diff),("borrow",borrow)))


def decoder1to2_dag() -> LogicDAG:
    """Enabled 1-to-2 decoder: y0=EN&!S, y1=EN&S."""
    b=DAGBuilder()
    en=b.input("en")
    s=b.input("s")
    ns=b.op(GateKind.NOT,s,name="not_s")
    y0=b.op(GateKind.AND,en,ns,name="y0")
    y1=b.op(GateKind.AND,en,s,name="y1")
    return b.finish((("y0",y0),("y1",y1)))
