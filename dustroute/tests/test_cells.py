from dustroute import *
from .common import settle


def test_not_cell():
    assert verify_not_cell()

def test_not_port_semantics():
    cell = make_not_cell()
    inp = cell.inputs[0]
    out = cell.outputs[0]
    assert inp.kind is PortKind.BLOCK_POWER
    assert cell.world.get(inp.pos).kind is BlockKind.SOLID
    torch_pos = Pos(1, 0, 0)
    torch = cell.world.get(torch_pos)
    assert torch.kind is BlockKind.REDSTONE_TORCH
    assert torch.support_pos(torch_pos) == inp.pos
    assert out.kind is PortKind.WIRE
    assert cell.world.get(out.pos).kind is BlockKind.REDSTONE_WIRE

def test_cell_library_candidates():
    lib = default_cell_library()
    nots = lib.candidates_for(GateKind.NOT)
    assert len(nots) >= 2
    verified = lib.verified_for(GateKind.NOT)
    assert len(verified) >= 2
    chosen = lib.choose(GateKind.NOT)
    assert chosen.name == 'not_torch_top'

def test_cell_library_verified_logic():
    lib = default_cell_library()
    assert all((v.valid for v in lib.verified_for(GateKind.OR)))
    assert all((v.valid for v in lib.verified_for(GateKind.AND)))

def test_nand_cell_verified():
    lib = default_cell_library()
    verified = lib.verified_for(GateKind.NAND)
    assert verified
    assert all((v.valid for v in verified))

def test_buffered_or_restores_true_output_to_15():
    c = make_or_buffered_cell()
    for a, b, expected in ((0, 0, 0), (0, 1, 15), (1, 0, 15), (1, 1, 15)):
        w = c.world.clone()
        if a:
            w.set(c.inputs[0].pos.offset(dx=-1), Block(BlockKind.REDSTONE_BLOCK))
        if b:
            w.set(c.inputs[1].pos.offset(dx=-1), Block(BlockKind.REDSTONE_BLOCK))
        update_wire_shapes(w)
        sim = RedstoneTickSimulator(w)
        state = sim.snapshot()
        for _ in range(4):
            state = sim.step()
        assert state.strength(c.outputs[0].pos) == expected
