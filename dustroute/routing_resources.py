from __future__ import annotations

from dataclasses import dataclass

from .model import Pos


def horizontal_neighbors(p: Pos) -> set[Pos]:
    return {
        p.offset(dx=1),p.offset(dx=-1),
        p.offset(dz=1),p.offset(dz=-1),
    }


def electrical_keepout_for_wire(p: Pos) -> set[Pos]:
    out=set(horizontal_neighbors(p))
    for q in tuple(horizontal_neighbors(p)):
        out.add(q.offset(dy=1))
        out.add(q.offset(dy=-1))
    return out


def branch_stair_clearances(branch: tuple[Pos,...]) -> set[Pos]:
    out=set()
    for a,b in zip(branch,branch[1:]):
        if a.y==b.y:
            continue
        lower=a if a.y<b.y else b
        out.add(lower.offset(dy=1))
    return out


@dataclass(frozen=True)
class RoutingResources:
    """Physical space/electrical reservations owned by routed interconnect."""
    conductors: frozenset[Pos]=frozenset()
    supports: frozenset[Pos]=frozenset()
    electrical_keepout: frozenset[Pos]=frozenset()
    stair_clearance: frozenset[Pos]=frozenset()
    terminals: frozenset[Pos]=frozenset()

    @classmethod
    def from_conductors(
        cls,
        conductors,
        *,
        stair_clearance=(),
        terminals=(),
    ):
        wire=set(conductors)
        keepout=set()
        for p in wire:
            keepout.update(electrical_keepout_for_wire(p))
        return cls(
            frozenset(wire),
            frozenset(p.offset(dy=-1) for p in wire),
            frozenset(keepout),
            frozenset(stair_clearance),
            frozenset(terminals),
        )

    def merged(self,*others:"RoutingResources") -> "RoutingResources":
        values=[self,*others]
        return RoutingResources(
            frozenset().union(*(x.conductors for x in values)),
            frozenset().union(*(x.supports for x in values)),
            frozenset().union(*(x.electrical_keepout for x in values)),
            frozenset().union(*(x.stair_clearance for x in values)),
            frozenset().union(*(x.terminals for x in values)),
        )

    @property
    def blocked_conductors(self) -> frozenset[Pos]:
        return frozenset(
            set(self.conductors)
            | set(self.electrical_keepout)
            | set(self.stair_clearance)
            | set(self.terminals)
        )
