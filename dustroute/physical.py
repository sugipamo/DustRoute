from __future__ import annotations
from dataclasses import dataclass,field
from .model import *
from .cells import PlacedCell, PortKind
from .routing import *
from .wire import update_wire_shapes
from .port_realization import terminal_for_endpoint

@dataclass(frozen=True)
class Node:
    id:int
    logical_kind:GateKind
    placed:PlacedCell

@dataclass(frozen=True)
class Endpoint:
    cell:int|None
    port:str
    pos:Pos
    kind:PortKind=PortKind.WIRE
    facing:Facing|None=None

@dataclass(frozen=True)
class Route:
    id:int
    source:Endpoint
    sink:Endpoint
    path:tuple[Pos,...]
    repeaters:tuple[Pos,...]=()

@dataclass(frozen=True)
class RewriteReport:
    rule:str
    removed_cells:tuple[int,...]
    removed_routes:tuple[int,...]
    added_routes:tuple[int,...]

@dataclass
class PhysicalCircuit:
    cells:dict[int,Node]=field(default_factory=dict)
    routes:dict[int,Route]=field(default_factory=dict)
    _nc:int=0
    _nr:int=0

    def add_cell(self,kind,placed):
        i=self._nc;self._nc+=1;self.cells[i]=Node(i,kind,placed);return i
    def boundary(self,name,pos,kind=PortKind.WIRE,facing=None):
        return Endpoint(None,name,pos,kind,facing)
    def input_ep(self,c,name):
        p=self.cells[c].placed.input_port(name)
        return Endpoint(c,name,p.pos,p.kind,p.facing)
    def output_ep(self,c,name):
        p=self.cells[c].placed.output_port(name)
        return Endpoint(c,name,p.pos,p.kind,p.facing)
    def add_route(self,s,t,path,reps=()):
        i=self._nr;self._nr+=1;self.routes[i]=Route(i,s,t,tuple(path),tuple(reps));return i
    def incoming(self,c):return tuple(r for r in self.routes.values() if r.sink.cell==c)
    def outgoing(self,c):return tuple(r for r in self.routes.values() if r.source.cell==c)

    def cell_world(self):
        w=World();occ=set()
        for n in self.cells.values():
            for p,b in n.placed.blocks():
                if p in occ:raise ValueError(f"overlap {p}")
                occ.add(p);w.set(p,b)
        return w

    def build_world(self):
        w=self.cell_world()
        for r in self.routes.values():
            reps=set(r.repeaters)
            for i,p in enumerate(r.path):
                # Cell port dust may already occupy path endpoints.
                if w.get(p).kind not in (BlockKind.AIR,BlockKind.REDSTONE_WIRE,BlockKind.REPEATER):continue
                sp=p.offset(dy=-1)
                if w.get(sp).kind is BlockKind.AIR:w.set(sp,Block(BlockKind.SOLID))
                if p in reps:
                    if 0<i<len(r.path)-1:
                        f=_facing(r.path[i-1],p)
                        if f:w.place(BlockKind.REPEATER,p.x,p.y,p.z,facing=f,delay=1)
                elif w.get(p).kind is BlockKind.AIR:
                    w.place(BlockKind.REDSTONE_WIRE,p.x,p.y,p.z)
        update_wire_shapes(w)
        return w

    def routing_world(self):
        """World containing cells and all currently retained routes."""
        return self.build_world()


def _facing(a,b):
    if a.y!=b.y:return None
    d=(b.x-a.x,b.z-a.z)
    return {(1,0):Facing.EAST,(-1,0):Facing.WEST,(0,1):Facing.SOUTH,(0,-1):Facing.NORTH}.get(d)


def _wire_terminal_for_endpoint(w,ep:Endpoint)->Pos:
    """Compatibility wrapper; port semantics live in port_realization.py."""
    return terminal_for_endpoint(w,ep)


def eliminate_double_not(pc,config=RouterConfig()):
    """Eliminate one NOT->NOT chain anywhere in the physical graph.

    The predecessor and successor may be boundaries or arbitrary cells. Their
    existing port positions become the stable boundary of the local rewrite.
    """
    for a,n in tuple(pc.cells.items()):
        if n.logical_kind is not GateKind.NOT:continue
        ai,ao=pc.incoming(a),pc.outgoing(a)
        if len(ai)!=1 or len(ao)!=1:continue
        mid=ao[0];b=mid.sink.cell
        if b is None or b not in pc.cells or pc.cells[b].logical_kind is not GateKind.NOT:continue
        bi,bo=pc.incoming(b),pc.outgoing(b)
        if len(bi)!=1 or len(bo)!=1 or bi[0].id!=mid.id:continue

        left,right=ai[0],bo[0]
        removed=(left.id,mid.id,right.id)
        source,sink=left.source,right.sink
        node_a=n
        node_b=pc.cells[b]
        old_routes={rid:pc.routes[rid] for rid in removed}

        # Remove only the local subgraph. Other cells/routes stay in the world
        # and therefore act as routing obstacles.
        for rid in removed:pc.routes.pop(rid,None)
        pc.cells.pop(a);pc.cells.pop(b)

        try:
            w=pc.routing_world()
            start=_wire_terminal_for_endpoint(w,source)
            goal=_wire_terminal_for_endpoint(w,sink)
            rr,reps=route_place_and_refresh(w,start,goal,config)
        except Exception:
            # Atomic rollback if replacement routing fails.
            pc.cells[a]=node_a
            pc.cells[b]=node_b
            pc.routes.update(old_routes)
            raise

        nr=pc.add_route(source,sink,rr.path,reps)
        return RewriteReport("double-not-elimination",(a,b),removed,(nr,))
    return None
