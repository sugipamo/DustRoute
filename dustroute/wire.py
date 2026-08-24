from __future__ import annotations
from .model import *

HORIZONTAL={Facing.NORTH:(0,0,-1),Facing.EAST:(1,0,0),Facing.SOUTH:(0,0,1),Facing.WEST:(-1,0,0)}

def add(p,d): return Pos(p.x+d[0],p.y+d[1],p.z+d[2])
def opposite(f): return {Facing.NORTH:Facing.SOUTH,Facing.SOUTH:Facing.NORTH,Facing.EAST:Facing.WEST,Facing.WEST:Facing.EAST,Facing.UP:Facing.DOWN,Facing.DOWN:Facing.UP}[f]
def hpos(p,f): return add(p,HORIZONTAL[f])

def _component_connects(block, direction):
    if block.kind in (BlockKind.LEVER,BlockKind.REDSTONE_TORCH,BlockKind.REDSTONE_BLOCK): return True
    if block.kind in (BlockKind.REPEATER,BlockKind.COMPARATOR): return block.facing in (direction,opposite(direction))
    return False

def infer_wire_connection(world,pos,facing):
    side=hpos(pos,facing); sb=world.get(side)
    if sb.kind is BlockKind.REDSTONE_WIRE or _component_connects(sb,facing): return WireConnection.SIDE
    above_side=side.offset(dy=1)
    if properties(sb.kind).supports_components and world.get(above_side).kind is BlockKind.REDSTONE_WIRE and world.get(pos.offset(dy=1)).kind is BlockKind.AIR:
        return WireConnection.UP
    below_side=side.offset(dy=-1)
    if sb.kind is BlockKind.AIR and world.get(below_side).kind is BlockKind.REDSTONE_WIRE: return WireConnection.SIDE
    return WireConnection.NONE

def resolved_wire_connection(world,pos,facing):
    b=world.get(pos)
    if b.kind is not BlockKind.REDSTONE_WIRE:return WireConnection.NONE
    explicit=b.wire_connection(facing)
    return infer_wire_connection(world,pos,facing) if explicit is None else explicit

def wire_has_arm(world,pos,facing): return resolved_wire_connection(world,pos,facing) is not WireConnection.NONE

def dust_connected(world,a,b):
    if world.get(a).kind is not BlockKind.REDSTONE_WIRE or world.get(b).kind is not BlockKind.REDSTONE_WIRE:return False
    for f,d in HORIZONTAL.items():
        side=add(a,d)
        if b==side: return wire_has_arm(world,a,f) and wire_has_arm(world,b,opposite(f))
        if b==side.offset(dy=1): return resolved_wire_connection(world,a,f) is WireConnection.UP
    for f,d in HORIZONTAL.items():
        side=add(b,d)
        if a==side.offset(dy=1): return resolved_wire_connection(world,b,f) is WireConnection.UP
    return False

def update_wire_shapes(world):
    updates=[]
    for pos,b in world.items():
        if b.kind is not BlockKind.REDSTONE_WIRE: continue
        con=[]
        for f in (Facing.NORTH,Facing.EAST,Facing.SOUTH,Facing.WEST):
            st=infer_wire_connection(world,pos,f)
            if st is not WireConnection.NONE: con.append((f,st))
        updates.append((pos,Block(b.kind,b.facing,b.powered,b.delay,b.support_offset,tuple(con))))
    for x in updates: world.set(*x)

def wire_shape_name(b):
    ds={f for f,s in (b.wire_connections or ()) if s is not WireConnection.NONE}
    if not ds:return "dot"
    if ds in ({Facing.NORTH,Facing.SOUTH},{Facing.EAST,Facing.WEST}):return "line"
    if len(ds)==2:return "corner"
    if len(ds)==3:return "tee"
    if len(ds)==4:return "cross"
    return "end"
