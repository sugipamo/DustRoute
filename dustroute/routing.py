from __future__ import annotations
from dataclasses import dataclass
from heapq import heappush,heappop
from math import inf
from .model import *
from .wire import update_wire_shapes

@dataclass(frozen=True)
class RouteResult:
    path:tuple[Pos,...]
    cost:float

@dataclass(frozen=True)
class RouterConfig:
    max_nodes:int=30000
    horizontal_cost:float=1.
    stair_cost:float=2.5
    new_support_cost:float=1.5

class RouteNotFound(RuntimeError): pass

def heuristic(a,b):return abs(a.x-b.x)+abs(a.y-b.y)+abs(a.z-b.z)

def moves(p):
    out=[]
    for dx,dz in ((1,0),(-1,0),(0,1),(0,-1)):
        for dy in (0,1,-1):out.append(Pos(p.x+dx,p.y+dy,p.z+dz))
    return out

def _routeable(world,p,start,goal):
    b=world.get(p)
    if p in (start,goal):
        return b.kind in (BlockKind.AIR,BlockKind.REDSTONE_WIRE)
    # Existing dust belongs to some already-routed net and is therefore an
    # obstacle. This prevents accidental shorts during local rerouting.
    return b.kind is BlockKind.AIR

def astar_route(world,start,goal,config=RouterConfig()):
    if not _routeable(world,start,start,goal):raise RouteNotFound(f"bad start {start}")
    if not _routeable(world,goal,start,goal):raise RouteNotFound(f"bad goal {goal}")
    pq=[(heuristic(start,goal),0,start)];serial=0;g={start:0.};prev={};n=0
    while pq:
        _,_,cur=heappop(pq)
        if cur==goal:
            path=[cur]
            while cur in prev:cur=prev[cur];path.append(cur)
            return RouteResult(tuple(reversed(path)),g[goal])
        n+=1
        if n>config.max_nodes:break
        for q in moves(cur):
            if not _routeable(world,q,start,goal):continue
            sp=q.offset(dy=-1);sb=world.get(sp)
            if sb.kind not in (BlockKind.AIR,BlockKind.SOLID,BlockKind.TRANSPARENT,BlockKind.REDSTONE_BLOCK):continue
            c=config.stair_cost if q.y!=cur.y else config.horizontal_cost
            if sb.kind is BlockKind.AIR:c+=config.new_support_cost
            ng=g[cur]+c
            if ng>=g.get(q,inf):continue
            g[q]=ng;prev[q]=cur;serial+=1;heappush(pq,(ng+heuristic(q,goal),serial,q))
    raise RouteNotFound(f"No route {start}->{goal}")

def _facing(a,b):
    if a.y!=b.y:return None
    d=(b.x-a.x,b.z-a.z)
    return {(1,0):Facing.EAST,(-1,0):Facing.WEST,(0,1):Facing.SOUTH,(0,-1):Facing.NORTH}.get(d)

def materialize_route(world,result,support_kind=BlockKind.SOLID):
    for p in result.path:
        sp=p.offset(dy=-1)
        if world.get(sp).kind is BlockKind.AIR:world.set(sp,Block(support_kind))
        if world.get(p).kind is BlockKind.AIR:world.place(BlockKind.REDSTONE_WIRE,p.x,p.y,p.z)
    update_wire_shapes(world)

def insert_repeaters(world,path,max_wire_run=14,delay=1):
    reps=[];run=0
    for i in range(1,len(path)-1):
        run+=1
        if run<max_wire_run:continue
        fi=_facing(path[i-1],path[i]);fo=_facing(path[i],path[i+1])
        if fi is None or fi!=fo:continue
        p=path[i]
        world.place(BlockKind.REPEATER,p.x,p.y,p.z,facing=fo,delay=delay)
        reps.append(p);run=0
    update_wire_shapes(world);return tuple(reps)

def route_place_and_refresh(world,start,goal,config=RouterConfig(),max_wire_run=14):
    r=astar_route(world,start,goal,config)
    materialize_route(world,r)
    reps=insert_repeaters(world,r.path,max_wire_run)
    return r,reps
