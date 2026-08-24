from dustroute import *
from .common import settle


def test_power_modes():
    w = World()
    w.fill(Pos(0, 0, 0), Pos(4, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.LEVER, 0, 1, 0, facing=Facing.EAST, powered=True, support_offset=Pos(0, -1, 0))
    w.place(BlockKind.REDSTONE_WIRE, 1, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE), (Facing.EAST, WireConnection.SIDE)))
    w.set(Pos(2, 1, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, 3, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE),))
    s = settle(w)
    assert s.weak_power(Pos(2, 1, 0)) > 0 and s.strength(Pos(3, 1, 0)) == 0
    w2 = World()
    w2.fill(Pos(0, 0, 0), Pos(5, 0, 0), Block(BlockKind.SOLID))
    w2.place(BlockKind.LEVER, 0, 1, 0, facing=Facing.EAST, powered=True, support_offset=Pos(0, -1, 0))
    w2.place(BlockKind.REDSTONE_WIRE, 1, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE), (Facing.EAST, WireConnection.SIDE)))
    w2.set(Pos(2, 1, 0), Block(BlockKind.SOLID))
    w2.place(BlockKind.REPEATER, 3, 1, 0, facing=Facing.EAST, delay=1)
    w2.place(BlockKind.REDSTONE_WIRE, 4, 1, 0, wire_connections=((Facing.WEST, WireConnection.SIDE),))
    s2 = settle(w2)
    assert s2.strength(Pos(4, 1, 0)) == 15

def test_electrical_properties_are_role_specific():
    solid = properties(BlockKind.SOLID)
    source = properties(BlockKind.REDSTONE_BLOCK)
    transparent = properties(BlockKind.TRANSPARENT)
    assert solid.receives_weak_power
    assert solid.receives_strong_power
    assert solid.repeater_reads_block_power
    assert solid.strong_power_drives_dust
    assert not source.can_be_powered
    assert not source.strong_power_drives_dust
    assert source.supports_components
    assert not transparent.can_be_powered
    assert not transparent.strong_power_drives_dust

def test_electrical_redstone_block_is_direct_source_only():
    w = World()
    w.set(Pos(-1, 0, 0), Block(BlockKind.REDSTONE_BLOCK))
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.set(Pos(-1, -1, 1), Block(BlockKind.SOLID))
    w.place(BlockKind.REDSTONE_WIRE, -1, 0, 1, wire_connections=((Facing.NORTH, WireConnection.SIDE),))
    state = RedstoneTickSimulator(w).snapshot()
    assert state.strength(Pos(-1, 0, 0)) == 15
    assert state.power(Pos(0, 0, 0)) == PoweredBlockState()
    assert state.strength(Pos(-1, 0, 1)) == 15

def test_electrical_lever_creates_stored_strong_power():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.LEVER, -1, 0, 0, facing=Facing.EAST, powered=True, support_offset=Pos(1, 0, 0))
    state = RedstoneTickSimulator(w).snapshot()
    assert state.weak_power(Pos(0, 0, 0)) == 0
    assert state.strong_power(Pos(0, 0, 0)) == 15

def test_instantaneous_settle_does_not_advance_torch():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.place(BlockKind.LEVER, -1, 0, 0, facing=Facing.EAST, powered=True, support_offset=Pos(1, 0, 0))
    w.place(BlockKind.REDSTONE_TORCH, 1, 0, 0, facing=Facing.EAST, support_offset=Pos(-1, 0, 0))
    sim = RedstoneTickSimulator(w)
    before = sim.snapshot()
    assert before.strong_power(Pos(0, 0, 0)) == 15
    assert before.strength(Pos(1, 0, 0)) == 15
    sim.settle_instantaneous()
    assert sim.snapshot().strength(Pos(1, 0, 0)) == 15
    after = sim.step()
    assert after.strength(Pos(1, 0, 0)) == 0

def test_torch_observes_support_block_state_not_adjacent_source():
    w = World()
    w.set(Pos(0, 0, 0), Block(BlockKind.SOLID))
    w.set(Pos(-1, 0, 0), Block(BlockKind.REDSTONE_BLOCK))
    w.place(BlockKind.REDSTONE_TORCH, 1, 0, 0, facing=Facing.EAST, support_offset=Pos(-1, 0, 0))
    sim = RedstoneTickSimulator(w)
    after = sim.step()
    assert after.power(Pos(0, 0, 0)) == PoweredBlockState()
    assert after.strength(Pos(1, 0, 0)) == 15
