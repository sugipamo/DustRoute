from __future__ import annotations
from dataclasses import dataclass

from .logic import Circuit, Direction, Gate, Net, Pin
from .model import GateKind, Pos, BlockKind, Block, World, Facing
from .cells import PhysicalCell, PlacedCell, RotationY, make_not_cell, make_terminal_cell
from .physical import PhysicalCircuit, Endpoint
from .routing import route_place_and_refresh
from .cell_library import default_cell_library


def half_adder() -> Circuit:
    """Logical half adder: sum=a XOR b, carry=a AND b."""
    gates=(
        Gate(0,GateKind.INPUT,0),
        Gate(1,GateKind.INPUT,0),
        Gate(2,GateKind.XOR,2),
        Gate(3,GateKind.AND,2),
        Gate(4,GateKind.OUTPUT,1),
        Gate(5,GateKind.OUTPUT,1),
    )
    nets=(
        Net(0,Pin(0,Direction.OUT,0),(Pin(2,Direction.IN,0),Pin(3,Direction.IN,0))),
        Net(1,Pin(1,Direction.OUT,0),(Pin(2,Direction.IN,1),Pin(3,Direction.IN,1))),
        Net(2,Pin(2,Direction.OUT,0),(Pin(4,Direction.IN,0),)),
        Net(3,Pin(3,Direction.OUT,0),(Pin(5,Direction.IN,0),)),
    )
    return Circuit(gates,nets)


def _clone_cell(name, world, inputs, outputs):
    return PhysicalCell(name,world,inputs,outputs)


def make_or_cell() -> PhysicalCell:
    """Simple 2-input dust OR with typed WIRE terminals."""
    from .cells import InputPort,OutputPort,PortKind
    from .wire import update_wire_shapes
    w=World()
    w.fill(Pos(0,0,0),Pos(4,0,2),Block(BlockKind.SOLID))
    for p in (Pos(0,1,0),Pos(1,1,0),Pos(2,1,0),Pos(2,1,1),
              Pos(0,1,2),Pos(1,1,2),Pos(2,1,2),Pos(3,1,1),Pos(4,1,1)):
        w.place(BlockKind.REDSTONE_WIRE,p.x,p.y,p.z)
    update_wire_shapes(w)
    return PhysicalCell("or_dust",w,
        (InputPort("a",Pos(0,1,0),PortKind.WIRE),InputPort("b",Pos(0,1,2),PortKind.WIRE)),
        (OutputPort("out",Pos(4,1,1),PortKind.WIRE),))



def make_or_buffered_cell() -> PhysicalCell:
    """Baseline OR with an explicit repeater output buffer.

    The original dust-only OR is logically correct but its output strength is
    whatever remains after the input routing and the OR's internal dust path.
    A compiler that treats gate boundaries as fresh logical Nets therefore
    cannot assume strength 15. This version establishes a digital cell
    contract: Boolean TRUE leaves the cell as a refreshed 15-strength wire.
    """
    from .cells import InputPort,OutputPort,PortKind
    from .wire import update_wire_shapes
    w=World()
    w.fill(Pos(0,0,0),Pos(5,0,2),Block(BlockKind.SOLID))
    for p in (
        Pos(0,1,0),Pos(1,1,0),Pos(2,1,0),Pos(2,1,1),
        Pos(0,1,2),Pos(1,1,2),Pos(2,1,2),Pos(3,1,1),
    ):
        w.place(BlockKind.REDSTONE_WIRE,p.x,p.y,p.z)
    w.place(BlockKind.REPEATER,4,1,1,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,5,1,1)
    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell(
        "or_dust_buffered",
        w,
        (
            InputPort("a",Pos(0,1,0),PortKind.WIRE,Facing.WEST),
            InputPort("b",Pos(0,1,2),PortKind.WIRE,Facing.WEST),
        ),
        (OutputPort("out",Pos(5,1,1),PortKind.WIRE,Facing.EAST),),
    )

def make_and_cell() -> PhysicalCell:
    """Verified baseline AND = NOT(OR(NOT(a), NOT(b))).

    The two input blocks each carry a torch inverter. Their outputs merge as
    dust, then a repeater strongly powers the final inverter support block.
    """
    from .cells import InputPort,OutputPort,PortKind
    from .wire import update_wire_shapes
    w=World()

    # Two BLOCK_POWER inputs and their attached inverter torches.
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.set(Pos(0,0,4),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_TORCH,1,0,0,facing=Facing.EAST,support_offset=Pos(-1,0,0))
    w.place(BlockKind.REDSTONE_TORCH,1,0,4,facing=Facing.EAST,support_offset=Pos(-1,0,0))

    # Supported dust merge of NOT(a) and NOT(b).
    w.fill(Pos(2,-1,0),Pos(3,-1,4),Block(BlockKind.SOLID))
    for z in range(5):
        w.place(BlockKind.REDSTONE_WIRE,2,0,z)
    w.place(BlockKind.REDSTONE_WIRE,3,0,2)

    # Repeater makes the final support block strongly powered whenever either
    # inverted input is high.
    w.set(Pos(4,-1,2),Block(BlockKind.SOLID))
    w.place(BlockKind.REPEATER,4,0,2,facing=Facing.EAST,delay=1)
    w.set(Pos(5,0,2),Block(BlockKind.SOLID))

    # Final inversion produces AND.
    w.place(BlockKind.REDSTONE_TORCH,6,0,2,facing=Facing.EAST,support_offset=Pos(-1,0,0))
    w.set(Pos(7,-1,2),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,7,0,2)

    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell("and_demorgan_repeater",w,
        (
            InputPort("a",Pos(0,0,0),PortKind.BLOCK_POWER,Facing.WEST),
            InputPort("b",Pos(0,0,4),PortKind.BLOCK_POWER,Facing.WEST),
        ),
        (OutputPort("out",Pos(7,0,2),PortKind.WIRE,Facing.EAST),))



def make_nand_cell() -> PhysicalCell:
    """Compact NAND = OR(NOT(a), NOT(b)) physical cell."""
    from .cells import InputPort,OutputPort,PortKind
    from .wire import update_wire_shapes

    w=World()

    # Two powered input blocks, each inverted by an attached torch.
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.set(Pos(0,0,4),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,1,0,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )
    w.place(
        BlockKind.REDSTONE_TORCH,1,0,4,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )

    # Merge NOT(a), NOT(b) directly as dust.
    w.fill(Pos(2,-1,0),Pos(3,-1,4),Block(BlockKind.SOLID))
    for z in range(5):
        w.place(BlockKind.REDSTONE_WIRE,2,0,z)
    w.place(BlockKind.REDSTONE_WIRE,3,0,2)

    update_wire_shapes(w)
    w.validate_supports()

    return PhysicalCell(
        "nand_torch_merge",
        w,
        (
            InputPort("a",Pos(0,0,0),PortKind.BLOCK_POWER,Facing.WEST),
            InputPort("b",Pos(0,0,4),PortKind.BLOCK_POWER,Facing.WEST),
        ),
        (OutputPort("out",Pos(3,0,2),PortKind.WIRE,Facing.EAST),),
    )

def make_xor_cell() -> PhysicalCell:
    """Temporary verified-by-truth-table macro: XOR built from redstone torches/dust.

    For the end-to-end compiler MVP this cell is intentionally represented as a
    black-box physical macro with two BLOCK_POWER inputs and one WIRE output.
    """
    # Reuse a compact layout assembled as (a OR b) AND NOT(a AND b) is future
    # work; for now the compiler can lower XOR logically before physical compile.
    raise NotImplementedError("XOR must be lowered before physical compilation")


def lower_xor(c: Circuit) -> Circuit:
    """Lower every binary XOR into OR(AND(a,NOT(b)), AND(NOT(a),b)).

    Rebuilds the circuit from signal-source references, preserving INPUT/OUTPUT
    boundaries. This is deliberately small and sufficient for half-adder MVP.
    """
    gates={g.id:g for g in c.gates}
    driver={}
    for n in c.nets:
        for s in n.sinks:driver[(s.gate,s.index)]=n.source.gate

    next_id=max(gates)+1 if gates else 0
    new_gates=[]
    # source token for each original gate output
    mapped={}
    connections=[] # (source_gid, sink_gid, sink_idx)

    def ng(kind,ins):
        nonlocal next_id
        i=next_id;next_id+=1;new_gates.append(Gate(i,kind,ins));return i

    # Topological enough for current acyclic generated circuits.
    pending=set(gates)
    while pending:
        progress=False
        for gid in list(pending):
            g=gates[gid]
            if g.kind is GateKind.INPUT:
                new_gates.append(g);mapped[gid]=gid;pending.remove(gid);progress=True;continue
            srcs=[driver.get((gid,i)) for i in range(g.input_count)]
            if any(s not in mapped for s in srcs):continue
            ms=[mapped[s] for s in srcs]
            if g.kind is GateKind.XOR:
                na=ng(GateKind.NOT,1);nb=ng(GateKind.NOT,1)
                aa=ng(GateKind.AND,2);bb=ng(GateKind.AND,2);oo=ng(GateKind.OR,2)
                connections += [(ms[1],na,0),(ms[0],nb,0),(ms[0],aa,0),(na,aa,1),
                                (nb,bb,0),(ms[1],bb,1),(aa,oo,0),(bb,oo,1)]
                mapped[gid]=oo
            else:
                nid=gid if gid not in {x.id for x in new_gates} else ng(g.kind,g.input_count)
                if nid==gid:new_gates.append(g)
                for i,s in enumerate(ms):connections.append((s,nid,i))
                mapped[gid]=nid
            pending.remove(gid);progress=True
        if not progress:raise ValueError("cycle/unconnected circuit")

    # group connections by source into hypernets
    by={}
    for s,d,i in connections:by.setdefault(s,[]).append(Pin(d,Direction.IN,i))
    nets=[];nid=0
    for s,sinks in by.items():
        nets.append(Net(nid,Pin(s,Direction.OUT,0),tuple(sinks)));nid+=1
    return Circuit(tuple(new_gates),tuple(nets))


CELL_FACTORIES={
    # Kept as a compatibility map; compilation now selects from CellLibrary.
    GateKind.NOT:make_not_cell,
    GateKind.OR:make_or_cell,
    GateKind.AND:make_and_cell,
    GateKind.INPUT:lambda:make_terminal_cell("input"),
    GateKind.OUTPUT:lambda:make_terminal_cell("output"),
}


@dataclass(frozen=True)
class CompileResult:
    logical:Circuit
    physical:PhysicalCircuit


def compile_circuit(c:Circuit, spacing_x=16, spacing_z=12) -> CompileResult:
    """Naive end-to-end compiler: lower XOR, layer-place cells, then route nets."""
    c=lower_xor(c)
    pc=PhysicalCircuit()
    nodes={}
    library=default_cell_library()
    # crude topological depth
    depth={g.id:0 for g in c.gates if g.kind is GateKind.INPUT}
    changed=True
    while changed:
        changed=False
        for n in c.nets:
            if n.source.gate not in depth:continue
            for s in n.sinks:
                d=depth[n.source.gate]+1
                if d>depth.get(s.gate,-1):depth[s.gate]=d;changed=True
    per_layer={}
    for g in c.gates:
        d=depth.get(g.id,0);idx=per_layer.get(d,0);per_layer[d]=idx+1
        cell=library.choose(g.kind)
        cid=pc.add_cell(g.kind,PlacedCell(cell,Pos(d*spacing_x,2,idx*spacing_z)))
        nodes[g.id]=cid

    # One route per sink for MVP. Shared-tree multi-net routing comes next.
    for n in c.nets:
        src_gate=next(g for g in c.gates if g.id==n.source.gate)
        src_c=nodes[n.source.gate]
        src=pc.output_ep(src_c,"out")
        for sinkpin in n.sinks:
            dst_c=nodes[sinkpin.gate]
            kind=next(g.kind for g in c.gates if g.id==sinkpin.gate)
            port="in" if kind is GateKind.OUTPUT else ("a" if sinkpin.index==0 else "b")
            dst=pc.input_ep(dst_c,port)
            # MVP routes each logical connection against cells. Multi-net
            # collision-aware routing is the next layer; keeping routes separate
            # gets the logical-to-physical pipeline end-to-end first.
            w=pc.cell_world()
            from .physical import _wire_terminal_for_endpoint
            start=_wire_terminal_for_endpoint(w,src);goal=_wire_terminal_for_endpoint(w,dst)
            rr,reps=route_place_and_refresh(w,start,goal)
            pc.add_route(src,dst,rr.path,reps)
    return CompileResult(c,pc)


def compile_circuit_multinet(c:Circuit, spacing_x=16, spacing_z=12):
    """Compile logical Circuit and route each logical Net as a shared tree."""
    from .multinet import route_all_nets, materialize_multinet

    c=lower_xor(c)
    pc=PhysicalCircuit()
    nodes={}
    library=default_cell_library()

    depth={g.id:0 for g in c.gates if g.kind is GateKind.INPUT}
    changed=True
    while changed:
        changed=False
        for n in c.nets:
            if n.source.gate not in depth:continue
            for s in n.sinks:
                d=depth[n.source.gate]+1
                if d>depth.get(s.gate,-1):
                    depth[s.gate]=d;changed=True

    per_layer={}
    gate_by_id={g.id:g for g in c.gates}
    for g in c.gates:
        d=depth.get(g.id,0); idx=per_layer.get(d,0); per_layer[d]=idx+1
        cid=pc.add_cell(
            g.kind,
            PlacedCell(library.choose(g.kind),Pos(d*spacing_x,2,idx*spacing_z))
        )
        nodes[g.id]=cid

    def endpoint_for_pin(pin):
        g=gate_by_id[pin.gate]
        cid=nodes[pin.gate]
        if pin.direction is Direction.OUT:
            return pc.output_ep(cid,"out")
        if g.kind is GateKind.OUTPUT:
            return pc.input_ep(cid,"in")
        return pc.input_ep(cid,"a" if pin.index==0 else "b")

    routing=route_all_nets(pc,c.nets,endpoint_for_pin)
    world=materialize_multinet(pc,routing)
    return c,pc,routing,world


def compile_circuit_ripup(
    c: Circuit,
    spacing_x=16,
    spacing_z=12,
    *,
    max_attempts=64,
    ripup_width=2,
):
    """End-to-end compile using shared-tree multi-Net rip-up/reroute."""
    from .multinet import route_all_nets_ripup, materialize_multinet

    c=lower_xor(c)
    pc=PhysicalCircuit()
    nodes={}
    library=default_cell_library()

    depth={g.id:0 for g in c.gates if g.kind is GateKind.INPUT}
    changed=True
    while changed:
        changed=False
        for n in c.nets:
            if n.source.gate not in depth:continue
            for s in n.sinks:
                d=depth[n.source.gate]+1
                if d>depth.get(s.gate,-1):
                    depth[s.gate]=d;changed=True

    per_layer={}
    gate_by_id={g.id:g for g in c.gates}
    for g in c.gates:
        d=depth.get(g.id,0)
        idx=per_layer.get(d,0)
        per_layer[d]=idx+1
        nodes[g.id]=pc.add_cell(
            g.kind,
            PlacedCell(
                library.choose(g.kind),
                Pos(d*spacing_x,2,idx*spacing_z),
            ),
        )

    def endpoint_for_pin(pin):
        g=gate_by_id[pin.gate]
        cid=nodes[pin.gate]
        if pin.direction is Direction.OUT:
            return pc.output_ep(cid,"out")
        if g.kind is GateKind.OUTPUT:
            return pc.input_ep(cid,"in")
        return pc.input_ep(cid,"a" if pin.index==0 else "b")

    result=route_all_nets_ripup(
        pc,c.nets,endpoint_for_pin,
        max_attempts=max_attempts,
        ripup_width=ripup_width,
    )
    world=materialize_multinet(pc,result.routing)
    return c,pc,result,world
