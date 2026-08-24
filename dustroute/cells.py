from __future__ import annotations
from dataclasses import dataclass
from enum import IntEnum, Enum, auto
from .model import *
from .sim import RedstoneTickSimulator
from .wire import update_wire_shapes

class PortKind(Enum):
    WIRE = auto()
    BLOCK_POWER = auto()

@dataclass(frozen=True)
class InputPort:
    name:str
    pos:Pos
    kind:PortKind=PortKind.WIRE
    facing:Facing|None=None

@dataclass(frozen=True)
class OutputPort:
    name:str
    pos:Pos
    kind:PortKind=PortKind.WIRE
    facing:Facing|None=None

@dataclass(frozen=True)
class CircuitFixture:
    world:World
    inputs:tuple[InputPort,...]
    outputs:tuple[OutputPort,...]

class RotationY(IntEnum):
    R0=0;R90=1;R180=2;R270=3

def rpos(p,r):
    return [Pos(p.x,p.y,p.z),Pos(-p.z,p.y,p.x),Pos(-p.x,p.y,-p.z),Pos(p.z,p.y,-p.x)][int(r)]

def rface(f,r):
    if f is None or f in (Facing.UP,Facing.DOWN):return f
    seq=[Facing.NORTH,Facing.EAST,Facing.SOUTH,Facing.WEST]
    return seq[(seq.index(f)+int(r))%4]

def transform_block(b,r):
    so=None if b.support_offset is None else rpos(b.support_offset,r)
    wc=None if b.wire_connections is None else tuple((rface(f,r),s) for f,s in b.wire_connections)
    return Block(b.kind,rface(b.facing,r),b.powered,b.delay,so,wc)

@dataclass(frozen=True)
class PhysicalCell:
    name:str
    world:World
    inputs:tuple[InputPort,...]
    outputs:tuple[OutputPort,...]

@dataclass(frozen=True)
class PlacedCell:
    cell:PhysicalCell
    origin:Pos
    rotation:RotationY=RotationY.R0
    def _tp(self,p):
        q=rpos(p,self.rotation)
        return q.offset(self.origin.x,self.origin.y,self.origin.z)
    def input_port(self,name):
        p=next(p for p in self.cell.inputs if p.name==name)
        return InputPort(p.name,self._tp(p.pos),p.kind,rface(p.facing,self.rotation))
    def output_port(self,name):
        p=next(p for p in self.cell.outputs if p.name==name)
        return OutputPort(p.name,self._tp(p.pos),p.kind,rface(p.facing,self.rotation))
    def input_pos(self,name):return self.input_port(name).pos
    def output_pos(self,name):return self.output_port(name).pos
    def input_facing(self,name):return self.input_port(name).facing
    def output_facing(self,name):return self.output_port(name).facing
    def blocks(self):return tuple((self._tp(p),transform_block(b,self.rotation)) for p,b in self.cell.world.items())


def make_not_cell():
    """Torch NOT whose input is explicitly a BLOCK_POWER port.

    The input port targets the opaque support block the torch is attached to.
    A router must therefore deliver power *to that block* rather than treating
    the NOT as an inline repeater-like component.
    """
    w=World()
    # The logical input target is this powered opaque block.
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))

    # Side-mounted torch depends on / observes the powered state of that block.
    w.place(
        BlockKind.REDSTONE_TORCH,1,0,0,
        facing=Facing.EAST,
        support_offset=Pos(-1,0,0),
    )

    # Ordinary wire output.
    w.set(Pos(2,-1,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,2,0,0)

    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell(
        "not_torch_block_power",
        w,
        (InputPort("a",Pos(0,0,0),PortKind.BLOCK_POWER,Facing.WEST),),
        (OutputPort("out",Pos(2,0,0),PortKind.WIRE,Facing.EAST),),
    )



def make_not_top_cell():
    """Alternative NOT: torch sits on top of its powered support block."""
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.place(
        BlockKind.REDSTONE_TORCH,0,1,0,
        facing=Facing.UP,
        support_offset=Pos(0,-1,0),
    )
    w.set(Pos(1,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,1,1,0)
    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell(
        "not_torch_top",
        w,
        (InputPort("a",Pos(0,0,0),PortKind.BLOCK_POWER,Facing.WEST),),
        (OutputPort("out",Pos(1,1,0),PortKind.WIRE,Facing.EAST),),
    )



def make_buffered_input_cell(name="buffered_input"):
    """
    Stable external INPUT boundary.

    External source -> input dust -> repeater -> output dust(15).

    The logical Net therefore always starts from a regenerated horizontal
    output instead of directly from a stimulus-adjacent terminal wire.
    """
    w=World()
    for x in range(3):
        w.set(Pos(x,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)
    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell(
        name,
        w,
        (InputPort("in",Pos(0,1,0),PortKind.WIRE,Facing.WEST),),
        (OutputPort("out",Pos(2,1,0),PortKind.WIRE,Facing.EAST),),
    )

def make_terminal_cell(name="terminal"):
    """One-wire physical endpoint useful as INPUT/OUTPUT or rewrite boundary."""
    w=World()
    w.set(Pos(0,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    update_wire_shapes(w)
    return PhysicalCell(
        name,w,
        (InputPort("in",Pos(0,1,0),PortKind.WIRE),),
        (OutputPort("out",Pos(0,1,0),PortKind.WIRE),),
    )


def verify_not_cell():
    """Truth-table check: external source powers the NOT support block."""
    for inp,expected in [(False,True),(True,False)]:
        c=make_not_cell()
        w=c.world.clone()
        # Lever is attached directly to the input/support block.
        w.place(
            BlockKind.LEVER,-1,0,0,
            facing=Facing.EAST,powered=inp,
            support_offset=Pos(1,0,0),
        )
        update_wire_shapes(w)
        sim=RedstoneTickSimulator(w)
        st=sim.snapshot()
        for _ in range(4):st=sim.step()
        if (st.strength(c.outputs[0].pos)>0)!=expected:return False
    return True


def make_buffered_output_cell(name="buffered_output"):
    """Observable output boundary with a repeater refresh stage.

    The physical input is ordinary dust. Any non-zero signal reaching it is
    regenerated to 15 before the externally observed output wire. This makes
    the compiler's Boolean output boundary explicit instead of assuming the
    incoming Net still has enough analog redstone strength to be visible.
    """
    w=World()
    w.fill(Pos(0,0,0),Pos(2,0,0),Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE,0,1,0)
    w.place(BlockKind.REPEATER,1,1,0,facing=Facing.EAST,delay=1)
    w.place(BlockKind.REDSTONE_WIRE,2,1,0)
    update_wire_shapes(w)
    w.validate_supports()
    return PhysicalCell(
        name,
        w,
        (InputPort("in",Pos(0,1,0),PortKind.WIRE,Facing.WEST),),
        (OutputPort("out",Pos(2,1,0),PortKind.WIRE,Facing.EAST),),
    )
