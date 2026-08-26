from __future__ import annotations
from dataclasses import dataclass, field
from heapq import heappush, heappop
from math import inf

from .model import Block, BlockKind, Facing, Pos, World
from .physical import Endpoint, PhysicalCircuit
from .routing import RouterConfig, RouteNotFound, _facing
from .wire import update_wire_shapes
from .connectivity import physical_step_connected
from .port_realization import terminal_for_endpoint, realize_sink_endpoint
from .routing_resources import (
    RoutingResources,
    electrical_keepout_for_wire,
    branch_stair_clearances,
)


@dataclass(frozen=True)
class RoutedNet:
    """
    Physical routing tree for one logical Net.

    `branches` stores one path per sink. Paths may share positions. `occupied`
    is the union of all tree positions and is the quantity reserved against
    other Nets.
    """
    net_id: int
    source: Endpoint
    sinks: tuple[Endpoint, ...]
    branches: tuple[tuple[Pos, ...], ...]
    occupied: frozenset[Pos]
    repeaters: frozenset[Pos] = frozenset()

    @property
    def wire_count(self) -> int:
        return len(self.occupied)


@dataclass
class MultiNetRouting:
    nets: dict[int, RoutedNet] = field(default_factory=dict)

    def occupied_by_other(self, net_id: int) -> set[Pos]:
        return set(self.resources(exclude_net=net_id).conductors)

    def resources(self, *, exclude_net: int | None = None) -> RoutingResources:
        resources=RoutingResources()
        for nid,net in self.nets.items():
            if exclude_net is not None and nid==exclude_net:
                continue
            resources=resources.merged(RoutingResources.from_conductors(
                net.occupied,
                stair_clearance=_net_stair_clearances(net),
            ))
        return resources



def _horizontal_neighbors(p: Pos) -> set[Pos]:
    return {
        p.offset(dx=1), p.offset(dx=-1),
        p.offset(dz=1), p.offset(dz=-1),
    }


def electrical_keepout_for_wire(p: Pos) -> set[Pos]:
    """
    Conservative cross-Net dust keepout.

    Minecraft dust can connect horizontally and can also form one-block
    up/down stair connections. We forbid another Net's wire in all of those
    candidate positions. This intentionally over-approximates connectivity:
    legality is preferred over compactness in the baseline router.
    """
    out=set(_horizontal_neighbors(p))
    for q in tuple(_horizontal_neighbors(p)):
        out.add(q.offset(dy=1))
        out.add(q.offset(dy=-1))
    return out


def _routing_resources(occupied: set[Pos] | frozenset[Pos]):
    """Compatibility adapter over RoutingResources."""
    r=RoutingResources.from_conductors(occupied)
    return set(r.conductors),set(r.supports),set(r.electrical_keepout)


def _cell_wire_keepout(
    world: World,
    allowed: set[Pos],
    *,
    exempt_cell_wires: set[Pos] | None = None,
) -> set[Pos]:
    """
    Keep routed dust away from unrelated cell-internal dust.

    Wires belonging to this Net's endpoint cells are exempt: the route must be
    able to enter/leave the cell through its declared port without the rest of
    that same cell electrically sealing the terminal.
    """
    exempt=set(exempt_cell_wires or ())
    blocked=set()
    for p,b in world.items():
        if b.kind is not BlockKind.REDSTONE_WIRE:
            continue
        if p in allowed or p in exempt:
            continue
        blocked.add(p)
        blocked.update(electrical_keepout_for_wire(p))
    return blocked


def _moves(p: Pos):
    for dx, dz in ((1,0),(-1,0),(0,1),(0,-1)):
        for dy in (0,1,-1):
            yield Pos(p.x+dx,p.y+dy,p.z+dz)


def _heuristic_to_tree(p: Pos, tree: set[Pos]) -> int:
    return min(abs(p.x-q.x)+abs(p.y-q.y)+abs(p.z-q.z) for q in tree)



def _partial_path_resources(
    cur: Pos,
    prev: dict[Pos,Pos],
) -> tuple[set[Pos],set[Pos]]:
    """Return (supports, stair_clearances) already committed on path to `cur`."""
    chain=[cur]
    q=cur
    while q in prev:
        q=prev[q]
        chain.append(q)

    supports={p.offset(dy=-1) for p in chain}
    clearances=set()
    for a,b in zip(chain,chain[1:]):
        if a.y==b.y:
            continue
        lower=a if a.y<b.y else b
        clearances.add(lower.offset(dy=1))
    return supports,clearances


def _route_branch_to_tree(
    world: World,
    start: Pos,
    tree: set[Pos],
    blocked: set[Pos],
    blocked_supports: set[Pos] | None = None,
    *,
    config: RouterConfig,
) -> tuple[Pos, ...]:
    """
    A* from one sink terminal to any point of this Net's existing tree.

    Existing positions in the same tree are goals and may be reused. Positions
    occupied by other Nets are blocked.
    """
    blocked_supports=set(blocked_supports or ())
    if start in blocked:
        raise RouteNotFound(f"sink terminal blocked by another net: {start}")

    pq=[(_heuristic_to_tree(start,tree),0,start)]
    g={start:0.0}; prev={}; serial=0; expanded=0

    # Supports required by already-routed same-Net tree positions are physical
    # blocks: a new conductor may not displace them.
    tree_supports={p.offset(dy=-1) for p in tree}
    tree_positions=set(tree)

    while pq:
        _,_,cur=heappop(pq)
        if cur in tree:
            path=[cur]
            while cur in prev:
                cur=prev[cur]; path.append(cur)
            return tuple(reversed(path))

        expanded += 1
        if expanded > config.max_nodes:
            break

        path_supports,path_clearances=_partial_path_resources(cur,prev)

        for q in _moves(cur):
            if q in blocked:
                continue

            support=q.offset(dy=-1)

            # A conductor may not occupy another Net's reserved support, and
            # its own required support may not overwrite another Net's wire.
            if q in blocked_supports:
                continue
            if support in blocked:
                continue

            # A conductor may not displace the support block required by an
            # already-routed same-Net tree position, nor may it sit directly
            # underneath a wire placed earlier on this very path.
            if q in path_supports or q in tree_supports:
                continue

            # Do not let this path destroy its own earlier stair geometry.
            if q in path_clearances or support in path_clearances:
                continue

            # Minecraft dust stair geometry needs an open head/side space.
            # A mere diagonal coordinate step is NOT sufficient.
            #
            # lower -> upper: space directly above the lower dust must be air.
            # upper -> lower: space directly above the lower destination must
            #                 likewise be air.
            if q.y != cur.y:
                lower = cur if cur.y < q.y else q
                clearance = lower.offset(dy=1)

                # The upper dust itself is allowed to occupy this position only
                # when it is exactly the destination/source. Otherwise a block
                # here prevents the stair arm.
                upper = q if q.y > cur.y else cur
                if clearance != upper:
                    if world.get(clearance).kind is not BlockKind.AIR:
                        continue
                    if clearance in blocked or clearance in blocked_supports:
                        continue

                # Existing same-Net tree conductors and supports are physical
                # volumes too and may not close the stair clearance.
                if clearance in tree_positions and clearance != upper:
                    continue
                if clearance in tree_supports and clearance != upper:
                    continue
                if clearance in path_supports and clearance != upper:
                    continue

            b=world.get(q)

            # Same-Net tree positions are valid targets even when materialized
            # as dust. Everywhere else, only AIR is routeable.
            if q not in tree and b.kind is not BlockKind.AIR:
                continue

            sb=world.get(support)
            if sb.kind not in (
                BlockKind.AIR, BlockKind.SOLID,
                BlockKind.TRANSPARENT, BlockKind.REDSTONE_BLOCK
            ):
                continue

            cost=config.stair_cost if q.y!=cur.y else config.horizontal_cost
            if sb.kind is BlockKind.AIR:
                cost += config.new_support_cost

            ng=g[cur]+cost
            if ng>=g.get(q,inf):
                continue
            g[q]=ng;prev[q]=cur;serial+=1
            heappush(pq,(ng+_heuristic_to_tree(q,tree),serial,q))

    raise RouteNotFound(f"cannot connect {start} to existing net tree")


def _insert_repeaters_on_branch(
    path: tuple[Pos,...],
    existing_repeaters: set[Pos],
    max_wire_run: int = 14,
) -> set[Pos]:
    """
    Conservative per-branch repeater placement. Shared-tree timing is more
    subtle; this inserts refreshers only on straight branch segments.
    """
    reps=set(existing_repeaters);run=0
    for i in range(1,len(path)-1):
        run+=1
        if run<max_wire_run:
            continue
        fi=_facing(path[i-1],path[i]);fo=_facing(path[i],path[i+1])
        if fi is None or fi!=fo:
            continue
        reps.add(path[i]);run=0
    return reps



def _tree_adjacency(branches: tuple[tuple[Pos,...], ...]) -> dict[Pos,set[Pos]]:
    adj:dict[Pos,set[Pos]]={}
    for branch in branches:
        for a,b in zip(branch,branch[1:]):
            adj.setdefault(a,set()).add(b)
            adj.setdefault(b,set()).add(a)
        if branch:
            adj.setdefault(branch[0],set())
    return adj


def _root_tree(
    source: Pos,
    branches: tuple[tuple[Pos,...], ...],
) -> tuple[dict[Pos,Pos|None],dict[Pos,set[Pos]]]:
    adj=_tree_adjacency(branches)
    if source not in adj:
        adj[source]=set()

    parent={source:None}
    queue=[source]
    for cur in queue:
        for q in adj.get(cur,()):
            if q in parent:
                continue
            parent[q]=cur
            queue.append(q)
    return parent,adj


def _path_from_root(parent: dict[Pos,Pos|None], sink: Pos) -> tuple[Pos,...]:
    if sink not in parent:
        raise RouteNotFound(f"sink {sink} is not connected to source tree")
    out=[sink]
    cur=sink
    while parent[cur] is not None:
        cur=parent[cur]
        out.append(cur)
    out.reverse()
    return tuple(out)


def _straight_tree_candidate(
    path: tuple[Pos,...],
    i: int,
    adj: dict[Pos,set[Pos]],
    forbidden: set[Pos],
) -> bool:
    if not (0 < i < len(path)-1):
        return False
    p=path[i]
    if p in forbidden:
        return False
    if len(adj.get(p,())) != 2:
        return False
    fi=_facing(path[i-1],p)
    fo=_facing(p,path[i+1])
    return fi is not None and fi == fo


def _plan_tree_repeaters(
    source: Pos,
    sinks: tuple[Pos,...],
    branches: tuple[tuple[Pos,...], ...],
    *,
    forbidden: set[Pos] | None = None,
    max_wire_run: int = 12,
) -> set[Pos]:
    """
    Plan signal refreshers against complete source->sink paths.

    Repeaters are never placed on turns, stairs, branch points, or cell-owned
    positions. A failure means the routed topology itself cannot satisfy the
    signal budget and must be rerouted/replaced rather than silently emitting
    an underpowered Minecraft circuit.
    """
    forbidden=set(forbidden or ())
    parent,adj=_root_tree(source,branches)
    paths=[_path_from_root(parent,s) for s in sinks]
    repeaters:set[Pos]=set()

    changed=True
    guard=0
    while changed:
        changed=False
        guard+=1
        if guard>1024:
            raise RouteNotFound("repeater planning did not converge")

        for path in paths:
            last_reset=0
            i=1
            while i < len(path):
                if path[i] in repeaters:
                    last_reset=i
                    i+=1
                    continue

                if i-last_reset <= max_wire_run:
                    i+=1
                    continue

                # Need a refresher no later than this point. Prefer the farthest
                # legal point from the previous reset to minimize count.
                candidate=None
                upper=i
                lower=last_reset+1
                for j in range(upper,lower-1,-1):
                    if _straight_tree_candidate(path,j,adj,forbidden):
                        candidate=j
                        break

                if candidate is None:
                    raise RouteNotFound(
                        f"no straight repeater site within signal budget "
                        f"between {path[last_reset]} and {path[i]}"
                    )

                repeaters.add(path[candidate])
                changed=True
                last_reset=candidate
                # Re-check from the new reset point.
                i=candidate+1

    # Final assertion.
    for path in paths:
        run=0
        for p in path[1:]:
            if p in repeaters:
                run=0
            else:
                run+=1
                if run>max_wire_run:
                    raise RouteNotFound(
                        f"signal budget still exceeded on path to {path[-1]}"
                    )

    return repeaters


def _repeater_facings(
    source: Pos,
    branches: tuple[tuple[Pos,...], ...],
    repeaters: set[Pos] | frozenset[Pos],
) -> dict[Pos,Facing]:
    parent,_=_root_tree(source,branches)
    out={}
    for p in repeaters:
        prev=parent.get(p)
        if prev is None:
            continue
        f=_facing(prev,p)
        if f is None:
            raise RouteNotFound(f"repeater {p} is not on a horizontal edge")
        out[p]=f
    return out


def _endpoint_approach(ep: Endpoint, terminal: Pos) -> Pos:
    """Compatibility wrapper; sink approach policy lives in port_realization."""
    delta={
        Facing.NORTH:(0,0,-1),Facing.EAST:(1,0,0),
        Facing.SOUTH:(0,0,1),Facing.WEST:(-1,0,0),
    }.get(ep.facing)
    return terminal if delta is None else terminal.offset(*delta)


def route_net_tree(
    pc: PhysicalCircuit,
    net_id: int,
    source: Endpoint,
    sinks: tuple[Endpoint,...],
    *,
    occupied_other: set[Pos] | None = None,
    reserved_other_terminals: set[Pos] | None = None,
    config: RouterConfig = RouterConfig(),
) -> RoutedNet:
    """
    Build a shared source -> N sinks tree while keeping every sink terminal a
    leaf. Typed ports with a horizontal facing receive a one-block approach
    stub, so later fan-out branches cannot change the terminal dust shape.
    """
    world=pc.cell_world()
    source_pos=terminal_for_endpoint(world,source)
    sink_realization=[realize_sink_endpoint(world,s) for s in sinks]
    sink_pos=[r.terminal for r in sink_realization]
    sink_approach=[r.approach for r in sink_realization]

    other_wire=set(occupied_other or ())
    other_wire_positions,other_supports,other_keepout=_routing_resources(other_wire)

    reserved=set(reserved_other_terminals or ())
    blocked=set(other_wire_positions)
    blocked.update(other_keepout)
    blocked.update(reserved)

    own_terminals={source_pos,*sink_pos,*sink_approach}

    endpoint_cell_ids={
        ep.cell
        for ep in (source,*sinks)
        if ep.cell is not None and ep.cell in pc.cells
    }
    exempt_cell_wires=set()
    for cid in endpoint_cell_ids:
        for p,b in pc.cells[cid].placed.blocks():
            if b.kind is BlockKind.REDSTONE_WIRE:
                exempt_cell_wires.add(p)

    blocked.update(_cell_wire_keepout(
        world,
        own_terminals,
        exempt_cell_wires=exempt_cell_wires,
    ))

    # Own terminals are un-blocked below so the route may enter/leave its
    # cells even when the declaration overlaps conservative reservations.
    # Precise legality (actual cross-Net dust contact, stair geometry,
    # signal budget) is enforced by the caller after each Net lands.
    for p in own_terminals:
        if p not in reserved:
            blocked.discard(p)

    # Sink terminals are leaves; the source remains the root of the fan-out
    # tree.
    joinable={source_pos}
    occupied_tree={source_pos}

    order=sorted(
        range(len(sinks)),
        key=lambda i:
            abs(sink_approach[i].x-source_pos.x)
            +abs(sink_approach[i].y-source_pos.y)
            +abs(sink_approach[i].z-source_pos.z)
    )

    routed_by_index={}
    own_stair_clearances=set()
    for i in order:
        terminal=sink_pos[i]
        approach=sink_approach[i]

        if approach == terminal:
            core=_route_branch_to_tree(
                world, terminal, joinable, blocked, other_supports, config=config
            )
            branch=tuple(reversed(core))
            joinable.update(branch[:-1])
        else:
            core=_route_branch_to_tree(
                world, approach, joinable, blocked, other_supports, config=config
            )
            branch=tuple(reversed(core)) + (terminal,)
            joinable.update(branch[:-1])

        routed_by_index[i]=branch
        occupied_tree.update(branch)

        # Later branches of the same Net may share conductors but must not
        # place conductor/support blocks into the air volume required by an
        # already-routed dust stair.
        new_clearances=branch_stair_clearances(branch)
        own_stair_clearances.update(new_clearances)
        blocked.update(new_clearances)

    branches=tuple(routed_by_index[i] for i in range(len(sinks)))

    forbidden_repeater_sites={
        p for p in occupied_tree
        if world.get(p).kind is not BlockKind.AIR
    }
    # Never replace the terminal or its approach stub with a repeater.
    forbidden_repeater_sites.update(sink_pos)
    forbidden_repeater_sites.update(sink_approach)

    repeaters=_plan_tree_repeaters(
        source_pos,
        tuple(sink_pos),
        branches,
        forbidden=forbidden_repeater_sites,
        max_wire_run=12,
    )

    return RoutedNet(
        net_id,source,sinks,branches,frozenset(occupied_tree),frozenset(repeaters)
    )



def _net_stair_clearances(net: RoutedNet) -> set[Pos]:
    out=set()
    for branch in net.branches:
        out.update(branch_stair_clearances(branch))
    return out


def route_all_nets(
    pc: PhysicalCircuit,
    logical_nets,
    endpoint_for_pin,
    *,
    config: RouterConfig = RouterConfig(),
) -> MultiNetRouting:
    """
    Sequential multi-Net router.

    Each completed Net reserves its occupied positions against later Nets.
    Net ordering currently prefers high fan-out first, then shorter endpoint
    span. A future rip-up/reroute pass can revisit poor ordering decisions.
    """
    routing=MultiNetRouting()

    jobs=[]
    world=pc.cell_world()
    terminals_by_net={}
    for n in logical_nets:
        source=endpoint_for_pin(n.source)
        sinks=tuple(endpoint_for_pin(s) for s in n.sinks)
        jobs.append((n,source,sinks))
        terminal_set={terminal_for_endpoint(world,source)}
        for s in sinks:
            realized=realize_sink_endpoint(world,s)
            terminal_set.add(realized.terminal)
            terminal_set.add(realized.approach)
        terminals_by_net[n.id]=terminal_set

    def span(job):
        _,source,sinks=job
        ps=[source.pos]+[s.pos for s in sinks]
        return (max(p.x for p in ps)-min(p.x for p in ps)
              + max(p.y for p in ps)-min(p.y for p in ps)
              + max(p.z for p in ps)-min(p.z for p in ps))

    jobs.sort(key=lambda j:(-len(j[2]),span(j)))

    for n,source,sinks in jobs:
        resources=routing.resources()
        occupied=set(resources.conductors)
        reserved_other=set(resources.stair_clearance)
        for other_id,terms in terminals_by_net.items():
            if other_id != n.id:
                reserved_other.update(terms)

        rn=route_net_tree(
            pc,n.id,source,sinks,
            occupied_other=occupied,
            reserved_other_terminals=reserved_other,
            config=config,
        )
        routing.nets[n.id]=rn

        # Precise cross-Net legality gate: geometric blocking cannot express
        # every Minecraft electrical rule (keepout is a conservative
        # over-approximation and terminal declarations may legally overlap
        # it). A Net that shorts or robs support from earlier geometry is
        # undone here, surfacing RouteNotFound for reorder/rip-up.
        contacts,support_conflicts=find_cross_net_conflicts(pc,routing)
        if contacts or support_conflicts:
            del routing.nets[n.id]
            raise RouteNotFound(
                f"net {n.id} produced illegal routing: "
                f"contacts={len(contacts)}, supports={len(support_conflicts)}"
            )

    return routing


def materialize_multinet(
    pc: PhysicalCircuit,
    routing: MultiNetRouting,
) -> World:
    """
    Materialize all RoutedNets into a single World.

    Other-Net overlap is rejected. Same-Net branch sharing is naturally allowed.
    """
    world=pc.cell_world()
    owner:dict[Pos,int]={}

    for nid,net in routing.nets.items():
        for p in net.occupied:
            previous=owner.get(p)
            if previous is not None and previous!=nid:
                raise ValueError(f"net short at {p}: {previous} vs {nid}")
            owner[p]=nid

            if world.get(p).kind is not BlockKind.AIR:
                # Existing cell dust is okay only at cell terminals.
                if world.get(p).kind is not BlockKind.REDSTONE_WIRE:
                    continue
            support=p.offset(dy=-1)
            if world.get(support).kind is BlockKind.AIR:
                world.set(support,Block(BlockKind.SOLID))

            if p in net.repeaters:
                # Repeater orientation is reconstructed below branch-wise.
                continue
            if world.get(p).kind is BlockKind.AIR:
                world.place(BlockKind.REDSTONE_WIRE,p.x,p.y,p.z)

        source_pos=terminal_for_endpoint(world,net.source)
        facings=_repeater_facings(
            source_pos,
            net.branches,
            set(net.repeaters),
        )
        for p,f in facings.items():
            world.place(
                BlockKind.REPEATER,p.x,p.y,p.z,
                facing=f,delay=1,
            )

    update_wire_shapes(world)
    return world


@dataclass(frozen=True)
class RerouteEvent:
    failed_net: int
    ripped_up: tuple[int, ...]
    attempt: int


@dataclass(frozen=True)
class RipupRoutingResult:
    routing: MultiNetRouting
    events: tuple[RerouteEvent, ...]
    attempts: int


def _routing_occupied(routing: MultiNetRouting) -> set[Pos]:
    occupied: set[Pos] = set()
    for net in routing.nets.values():
        occupied.update(net.occupied)
    return occupied


def route_all_nets_ripup(
    pc: PhysicalCircuit,
    logical_nets,
    endpoint_for_pin,
    *,
    config: RouterConfig = RouterConfig(),
    max_attempts: int = 32,
    ripup_width: int = 2,
) -> RipupRoutingResult:
    """
    Sequential routing with bounded rip-up & reroute.

    On failure:
      1. remove up to `ripup_width` most recently routed Nets,
      2. move the failed Net in front of those ripped Nets,
      3. retry the freed region,
      4. continue until all Nets route or `max_attempts` is exhausted.

    This is intentionally a small negotiated-congestion precursor. It already
    removes the strongest ordering dependence of the original greedy router
    without baking signal strength into topology.
    """
    if ripup_width < 1:
        raise ValueError("ripup_width must be >= 1")
    if max_attempts < 1:
        raise ValueError("max_attempts must be >= 1")

    jobs = []
    for n in logical_nets:
        source = endpoint_for_pin(n.source)
        sinks = tuple(endpoint_for_pin(s) for s in n.sinks)
        jobs.append((n, source, sinks))

    def span(job):
        _, source, sinks = job
        ps = [source.pos] + [s.pos for s in sinks]
        return (
            max(p.x for p in ps) - min(p.x for p in ps)
            + max(p.y for p in ps) - min(p.y for p in ps)
            + max(p.z for p in ps) - min(p.z for p in ps)
        )

    # Initial ordering remains the useful greedy heuristic.
    jobs.sort(key=lambda j: (-len(j[2]), span(j)))

    routing = MultiNetRouting()
    routed_order: list[int] = []
    job_by_id = {job[0].id: job for job in jobs}

    terminal_by_net: dict[int, set[Pos]] = {}
    base_world = pc.cell_world()
    for n, source, sinks in jobs:
        terminal_set={terminal_for_endpoint(base_world,source)}
        for s in sinks:
            realized=realize_sink_endpoint(base_world,s)
            terminal_set.add(realized.terminal)
            terminal_set.add(realized.approach)
        terminal_by_net[n.id]=terminal_set

    pending = [job[0].id for job in jobs]
    events: list[RerouteEvent] = []
    attempts = 0

    while pending:
        if attempts >= max_attempts:
            raise RouteNotFound(
                f"rip-up router exhausted {max_attempts} attempts; "
                f"remaining nets={pending}"
            )
        attempts += 1

        nid = pending.pop(0)
        n, source, sinks = job_by_id[nid]

        try:
            reserved_other = set()
            for other_id, terminals in terminal_by_net.items():
                if other_id != nid:
                    reserved_other.update(terminals)
            for old in routing.nets.values():
                reserved_other.update(_net_stair_clearances(old))

            rn = route_net_tree(
                pc,
                nid,
                source,
                sinks,
                occupied_other=_routing_occupied(routing),
                reserved_other_terminals=reserved_other,
                config=config,
            )
            routing.nets[nid] = rn

            # Same precise cross-Net legality gate as the plain sequential
            # router, limited to the violation classes that re-ordering can
            # actually fix (shorts and support theft). Per-Net continuity and
            # signal budget remain the caller's final acceptance gate.
            contacts,support_conflicts=find_cross_net_conflicts(pc,routing)
            if contacts or support_conflicts:
                del routing.nets[nid]
                raise RouteNotFound(
                    f"net {nid} produced illegal routing: "
                    f"contacts={len(contacts)}, supports={len(support_conflicts)}"
                )

            routed_order.append(nid)
            continue

        except RouteNotFound:
            if not routed_order:
                raise

            # Rip up the most recent nets. The failed net gets first claim on
            # the freed region, then ripped nets are requeued behind it.
            count = min(ripup_width, len(routed_order))
            ripped = tuple(routed_order[-count:])
            del routed_order[-count:]

            for old_id in ripped:
                routing.nets.pop(old_id, None)

            events.append(
                RerouteEvent(
                    failed_net=nid,
                    ripped_up=ripped,
                    attempt=attempts,
                )
            )

            # failed first, then the displaced nets, then untouched pending.
            pending = [nid, *ripped, *pending]

            # Avoid immediate infinite cycles by rotating a repeatedly failing
            # configuration: if the last two events are identical, reverse the
            # ripped order on the next retry.
            if (
                len(events) >= 2
                and events[-1].failed_net == events[-2].failed_net
                and events[-1].ripped_up == events[-2].ripped_up
            ):
                pending = [nid, *reversed(ripped), *pending[1 + len(ripped):]]

    return RipupRoutingResult(routing, tuple(events), attempts)


@dataclass(frozen=True)
class RoutingLegalityReport:
    cross_net_contacts: tuple[tuple[int,int,Pos,Pos], ...]
    support_conflicts: tuple[tuple[int,int,Pos], ...]
    over_budget_paths: tuple[tuple[int,int,int], ...]
    broken_steps: tuple[BrokenRouteStep, ...] = ()

    @property
    def valid(self) -> bool:
        return (
            not self.cross_net_contacts
            and not self.support_conflicts
            and not self.over_budget_paths
            and not self.broken_steps
        )


def find_cross_net_conflicts(
    pc: PhysicalCircuit,
    routing: MultiNetRouting,
    world: World | None = None,
):
    """
    Precise but cheap cross-Net violation scan.

    Returns (contacts, support_conflicts) using the same rules as
    :func:`validate_routing_legality`, without reconstructing per-Net source
    to sink paths. Safe to run after every routed Net.
    """
    world=world or materialize_multinet(pc,routing)

    owner:dict[Pos,int]={}
    for nid,net in routing.nets.items():
        for p in net.occupied:
            owner.setdefault(p,nid)

    cell_boxes=[]
    for node in pc.cells.values():
        ps=[p for p,_ in node.placed.blocks()]
        if not ps:
            continue
        lo=Pos(min(p.x for p in ps),min(p.y for p in ps),min(p.z for p in ps))
        hi=Pos(max(p.x for p in ps),max(p.y for p in ps),max(p.z for p in ps))
        cell_boxes.append((lo,hi))

    def same_cell_volume(a: Pos,b: Pos) -> bool:
        for lo,hi in cell_boxes:
            def inside(p):
                return (
                    lo.x<=p.x<=hi.x
                    and lo.y<=p.y<=hi.y
                    and lo.z<=p.z<=hi.z
                )
            if inside(a) and inside(b):
                return True
        return False

    from .wire import dust_connected
    contacts=set()
    for p,nid in owner.items():
        if world.get(p).kind is not BlockKind.REDSTONE_WIRE:
            continue
        for q in electrical_keepout_for_wire(p):
            oid=owner.get(q)
            if oid is None or oid==nid:
                continue
            if world.get(q).kind is not BlockKind.REDSTONE_WIRE:
                continue
            if same_cell_volume(p,q):
                continue
            if dust_connected(world,p,q):
                a,b=sorted((nid,oid))
                pp,qq=(p,q) if nid==a else (q,p)
                contacts.add((a,b,pp,qq))

    support_conflicts=set()
    for a,na in routing.nets.items():
        supports={p.offset(dy=-1) for p in na.occupied}
        for b,nb in routing.nets.items():
            if a>=b:
                continue
            for p in supports.intersection(nb.occupied):
                support_conflicts.add((a,b,p))
            bs={p.offset(dy=-1) for p in nb.occupied}
            for p in bs.intersection(na.occupied):
                support_conflicts.add((a,b,p))

    return (
        tuple(sorted(contacts,key=lambda x:(x[0],x[1],x[2].x,x[2].y,x[2].z))),
        tuple(sorted(support_conflicts,key=lambda x:(x[0],x[1],x[2].x,x[2].y,x[2].z))),
    )


def validate_routing_legality(
    pc: PhysicalCircuit,
    routing: MultiNetRouting,
    world: World | None = None,
    *,
    max_wire_run: int = 12,
) -> RoutingLegalityReport:
    """
    Validate only interconnect legality; intentional connectivity *inside*
    physical logic cells is not mistaken for a routing short.
    """
    world=world or materialize_multinet(pc,routing)

    # Direct Minecraft dust contacts between positions owned by different Nets.
    owner:dict[Pos,int]={}
    for nid,net in routing.nets.items():
        for p in net.occupied:
            owner.setdefault(p,nid)

    # Bounding volumes of physical logic cells. Different logical Nets are
    # allowed to meet inside a cell (OR is the obvious example); that is cell
    # semantics, not an interconnect short.
    cell_boxes=[]
    for node in pc.cells.values():
        ps=[p for p,_ in node.placed.blocks()]
        if not ps:
            continue
        lo=Pos(min(p.x for p in ps),min(p.y for p in ps),min(p.z for p in ps))
        hi=Pos(max(p.x for p in ps),max(p.y for p in ps),max(p.z for p in ps))
        cell_boxes.append((lo,hi))

    def same_cell_volume(a: Pos,b: Pos) -> bool:
        for lo,hi in cell_boxes:
            def inside(p):
                return (
                    lo.x<=p.x<=hi.x
                    and lo.y<=p.y<=hi.y
                    and lo.z<=p.z<=hi.z
                )
            if inside(a) and inside(b):
                return True
        return False

    contacts=set()
    from .wire import dust_connected
    for p,nid in owner.items():
        if world.get(p).kind is not BlockKind.REDSTONE_WIRE:
            continue
        for q in electrical_keepout_for_wire(p):
            oid=owner.get(q)
            if oid is None or oid==nid:
                continue
            if world.get(q).kind is not BlockKind.REDSTONE_WIRE:
                continue
            if same_cell_volume(p,q):
                continue
            if dust_connected(world,p,q):
                a,b=sorted((nid,oid))
                pp,qq=(p,q) if nid==a else (q,p)
                contacts.add((a,b,pp,qq))

    # A Net's generated support must not overwrite another Net's conductor.
    support_conflicts=set()
    for a,na in routing.nets.items():
        supports={p.offset(dy=-1) for p in na.occupied}
        for b,nb in routing.nets.items():
            if a>=b:
                continue
            for p in supports.intersection(nb.occupied):
                support_conflicts.add((a,b,p))
            bs={p.offset(dy=-1) for p in nb.occupied}
            for p in bs.intersection(na.occupied):
                support_conflicts.add((a,b,p))

    # Reconstruct each complete source->sink tree path and verify refresh budget.
    over=[]
    base=pc.cell_world()
    for nid,net in routing.nets.items():
        source=terminal_for_endpoint(base,net.source)
        sinks=tuple(terminal_for_endpoint(base,s) for s in net.sinks)
        parent,_=_root_tree(source,net.branches)
        for si,sink in enumerate(sinks):
            path=_path_from_root(parent,sink)
            run=0
            worst=0
            for p in path[1:]:
                if p in net.repeaters:
                    run=0
                else:
                    run+=1
                    worst=max(worst,run)
            if worst>max_wire_run:
                over.append((nid,si,worst))

    continuity=validate_route_continuity(pc,routing,world)

    return RoutingLegalityReport(
        tuple(sorted(contacts,key=lambda x:(x[0],x[1],x[2].x,x[2].y,x[2].z))),
        tuple(sorted(support_conflicts,key=lambda x:(x[0],x[1],x[2].x,x[2].y,x[2].z))),
        tuple(over),
        continuity.broken,
    )


@dataclass(frozen=True)
class BrokenRouteStep:
    net_id: int
    branch_index: int
    step_index: int
    src: Pos
    dst: Pos


@dataclass(frozen=True)
class RouteContinuityReport:
    broken: tuple[BrokenRouteStep, ...]

    @property
    def valid(self) -> bool:
        return not self.broken


def validate_route_continuity(
    pc: PhysicalCircuit,
    routing: MultiNetRouting,
    world: World | None = None,
) -> RouteContinuityReport:
    """
    Check every directed adjacent pair stored in every routed branch against
    the actual materialized Minecraft connection rule.

    This catches a class of bugs that geometric routing checks cannot:
    - repeater facing the wrong way,
    - dust that is adjacent but not shape-connected,
    - invalid stair transitions,
    - a visible one-block gap represented incorrectly by a route.
    """
    world=world or materialize_multinet(pc,routing)
    broken=[]

    for nid,net in sorted(routing.nets.items()):
        for bi,branch in enumerate(net.branches):
            for si,(a,b) in enumerate(zip(branch,branch[1:])):
                if not physical_step_connected(world,a,b):
                    broken.append(BrokenRouteStep(nid,bi,si,a,b))

    return RouteContinuityReport(tuple(broken))
