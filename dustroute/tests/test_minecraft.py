from dustroute import *
from .common import settle


def test_java_export_block_states():
    c = make_not_cell()
    cmds = world_setblock_commands(c.world, JavaExportConfig(namespace='test'))
    joined = '\\n'.join(cmds)
    assert 'minecraft:redstone_wall_torch[facing=east,lit=true]' in joined
    assert 'minecraft:redstone_wire[' in joined
    assert 'minecraft:stone' in joined

def test_java_export_cell_pack():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_cell_mcfunctions(make_not_cell(), out, truth_fn=lambda i: not i['a'], config=JavaExportConfig(namespace='ro_test', pack_format=71, test_delay_ticks=4))
        assert (out / 'pack.mcmeta').exists()
        assert (out / 'data/ro_test/function/build.mcfunction').exists()
        assert (out / 'data/ro_test/function/cases/0.mcfunction').exists()
        assert (out / 'data/ro_test/function/cases/1.mcfunction').exists()
        case = (out / 'data/ro_test/function/cases/0.mcfunction').read_text()
        check = (out / 'data/ro_test/function/checks/0.mcfunction').read_text()
        assert 'schedule function ro_test:checks/0 4t replace' in case
        assert 'redstone_wire[power=0]' in check

def test_gate_test_datapack_api():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_gate_test_datapack(out, config=JavaExportConfig(namespace='ro_test', pack_format=71, test_delay_ticks=4))
        assert (out / 'data/ro_test/function/build.mcfunction').read_text().strip() == 'function ro_test:not/build'
        for gate in ('not', 'or', 'and', 'nand'):
            assert (out / f'data/ro_test/function/build_{gate}.mcfunction').exists()
            build = (out / f'data/ro_test/function/{gate}/build.mcfunction').read_text()
            assert 'summon minecraft:marker' in build
            assert f'tag=ro_test_{gate}_origin' in build
        assert (out / 'data/ro_test/function/not/cases/0.mcfunction').exists()
        assert (out / 'data/ro_test/function/not/cases/1.mcfunction').exists()
        for bits in ('00', '01', '10', '11'):
            assert (out / f'data/ro_test/function/and/cases/{bits}.mcfunction').exists()
            assert (out / f'data/ro_test/function/or/cases/{bits}.mcfunction').exists()
            assert (out / f'data/ro_test/function/nand/cases/{bits}.mcfunction').exists()
        case = (out / 'data/ro_test/function/and/cases/01.mcfunction').read_text()
        check = (out / 'data/ro_test/function/and/checks/01.mcfunction').read_text()
        assert 'execute at @e[type=minecraft:marker,tag=ro_test_and_origin' in case
        assert 'execute at @e[type=minecraft:marker,tag=ro_test_and_origin' in check

def test_java_repeater_facing_conversion():
    b = Block(BlockKind.REPEATER, facing=Facing.EAST, delay=1)
    state = java_block_state(b)
    assert 'facing=west' in state

def test_java_comparator_facing_conversion():
    b = Block(BlockKind.COMPARATOR, facing=Facing.SOUTH)
    state = java_block_state(b)
    assert 'facing=north' in state

def test_isolated_case_rebuild_commands():
    cfg = JavaExportConfig(namespace='ro_test', pack_format=71)
    cell = make_not_cell()
    cmds = isolated_build_commands(cell.world, cfg)
    assert cmds[0].startswith('fill ') and 'minecraft:air replace' in cmds[0]
    assert any(('minecraft:stone replace' in c and c.startswith('fill ') for c in cmds[1:]))
    assert any(('minecraft:redstone_wall_torch' in c for c in cmds))

def test_gate_case_is_self_contained():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_gate_test_datapack(out, config=JavaExportConfig(namespace='ro_test', pack_format=71))
        case = (out / 'data/ro_test/function/and/_cases/01.mcfunction').read_text()
        lines = case.splitlines()
        assert lines[0].startswith('fill ') and 'minecraft:air replace' in lines[0]
        assert any((line.startswith('fill ') and 'minecraft:stone replace' in line for line in lines[1:]))
        assert any(('minecraft:repeater' in line for line in lines))
        assert any(('minecraft:lever' in line for line in lines))

def test_gallery_builds_all_gates():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_gate_test_datapack(out, config=JavaExportConfig(namespace='ro_test', pack_format=71))
        tests = (out / 'data/ro_test/function/tests.mcfunction').read_text()
        assert 'function ro_test:not/build' in tests
        assert 'function ro_test:or/build' in tests
        assert 'function ro_test:and/build' in tests
        assert 'function ro_test:nand/build' in tests
        assert 'Starting automatic truth-table tests' in tests
        assert 'function ro_test:tests_run' in tests
        for gate in ('not', 'or', 'and', 'nand'):
            build = (out / f'data/ro_test/function/{gate}/build.mcfunction').read_text()
            assert f'tag=ro_test_{gate}_origin' in build

def test_gallery_tests_run_all_cases():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_gate_test_datapack(out, config=JavaExportConfig(namespace='ro_test', pack_format=71, test_delay_ticks=4))
        run = (out / 'data/ro_test/function/tests_run.mcfunction').read_text()
        assert 'function ro_test:not/cases/0' in run
        assert 'schedule function ro_test:not/cases/1 6t replace' in run
        assert 'schedule function ro_test:and/cases/11' in run
        assert 'schedule function ro_test:nand/cases/11' in run

def test_safe_reset_removes_components_before_supports():
    cfg = JavaExportConfig(namespace='ro_test', pack_format=71)
    cmds = reset_test_region_commands(make_and_cell().world, cfg)
    text = '\\n'.join(cmds)
    wire_i = next((i for i, c in enumerate(cmds) if 'replace minecraft:redstone_wire' in c))
    repeater_i = next((i for i, c in enumerate(cmds) if 'replace minecraft:repeater' in c))
    full_clear_i = next((i for i, c in enumerate(cmds) if c.endswith('minecraft:air replace')))
    assert wire_i < full_clear_i
    assert repeater_i < full_clear_i
    assert any(('kill @e[type=minecraft:item' in c for c in cmds))

def test_case_uses_safe_reset():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_gate_test_datapack(out, config=JavaExportConfig(namespace='ro_test', pack_format=71))
        case = (out / 'data/ro_test/function/and/_cases/01.mcfunction').read_text()
        lines = case.splitlines()
        assert 'replace minecraft:redstone_wire' in lines[0]
        assert any(('replace minecraft:repeater' in x for x in lines[:8]))
        assert any(('kill @e[type=minecraft:item' in x for x in lines[:10]))

def test_semantics_probe_catalog():
    cfg = JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20)
    probes = semantic_probes(cfg)
    assert [p.name for p in probes] == ['01_source_to_dust', '02_dust_decay', '03_weak_block_no_dust_return', '04_weak_block_to_repeater', '05_repeater_refresh', '06_repeater_strong_block', '07_torch_unpowered_support', '08_torch_powered_support', '09_redstone_block_no_block_propagation', '10_dust_repeater_dust', '11_repeater_reverse_blocked', '12_dust_to_repeater_input', '13_dust_corner', '14_dust_stair_up', '15_dust_stair_down', '16_leaf_dust_block_power', '17_branch_leaf_block_power', '18_repeater_route_block_power', '19_cell_output_to_block_input', '20_repeater_to_corner']
    for probe in probes:
        probe.world.validate_supports()

def test_semantics_datapack_api():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_semantics_datapack(out, config=JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20))
        tests = (out / 'data/ro_sem/function/tests.mcfunction').read_text()
        assert 'function ro_sem:_launch_01_source_to_dust' in tests
        assert (out / 'data/ro_sem/function/_launch_08_torch_powered_support.mcfunction').exists()
        assert 'function ro_sem:_launch_01_source_to_dust' in tests
        assert (out / 'data/ro_sem/function/help.mcfunction').exists()

def test_semantics_probe_03_has_no_direct_dust_stair():
    cfg = JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20)
    probe = next((p for p in semantic_probes(cfg) if p.name == '03_weak_block_no_dust_return'))
    assert probe.world.get(Pos(1, 1, 0)).kind is BlockKind.REDSTONE_WIRE
    assert probe.world.get(Pos(0, 1, 0)).kind is BlockKind.SOLID
    assert probe.world.get(Pos(-1, 1, 0)).kind is BlockKind.REDSTONE_WIRE
    assert not dust_connected(probe.world, Pos(1, 1, 0), Pos(-1, 1, 0))

def test_semantics_phased_api():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_semantics_datapack(out, config=JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20), build_settle_ticks=4)
        pdir = out / 'data/ro_sem/function/06_repeater_strong_block'
        run = (pdir / 'run.mcfunction').read_text()
        build = (pdir / '_build.mcfunction').read_text()
        stim = (pdir / '_stimulate.mcfunction').read_text()
        assert 'redstone_block' not in build
        assert 'redstone_block' in stim
        assert 'stimulate 4t replace' in run
        assert 'check 24t replace' in run
        assert (pdir / 'build.mcfunction').exists()
        assert (pdir / 'stimulate.mcfunction').exists()
        assert (pdir / 'check.mcfunction').exists()

def test_semantics_suite_uses_persistent_origin():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_semantics_datapack(out, config=JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20), build_settle_ticks=4)
        tests = (out / 'data/ro_sem/function/tests.mcfunction').read_text()
        assert 'summon minecraft:marker ~ ~ ~ {Tags:["ro_sem_suite_origin"]}' in tests
        launch = (out / 'data/ro_sem/function/_launch_04_weak_block_to_repeater.mcfunction').read_text()
        assert 'execute at @e[type=minecraft:marker,tag=ro_sem_suite_origin,limit=1]' in launch
        assert 'positioned ~' not in launch
        assert 'run function ro_sem:04_weak_block_to_repeater/run' in launch

def test_semantics_probe_08_uses_dust_powered_support():
    cfg = JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20)
    probe = next((p for p in semantic_probes(cfg) if p.name == '08_torch_powered_support'))
    stim = '\\n'.join(probe.stimulus)
    assert 'minecraft:redstone_block' in stim
    assert 'minecraft:lever' not in stim
    assert probe.world.get(Pos(-1, 1, 0)).kind is BlockKind.REDSTONE_WIRE
    assert probe.world.get(Pos(0, 1, 0)).kind is BlockKind.SOLID
    torch = probe.world.get(Pos(1, 1, 0))
    assert torch.kind is BlockKind.REDSTONE_TORCH
    assert torch.support_pos(Pos(1, 1, 0)) == Pos(0, 1, 0)
    assert probe.world.get(Pos(-2, 1, 0)).kind is BlockKind.AIR
    assert any(('dust -> support -> torch OFF' in c for c in probe.checks))

def test_semantics_probe_09_redstone_block_no_propagation():
    cfg = JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20)
    probe = next((p for p in semantic_probes(cfg) if p.name == '09_redstone_block_no_block_propagation'))
    stim = '\\n'.join(probe.stimulus)
    assert 'minecraft:redstone_block' in stim
    assert any(('torch stays ON' in c for c in probe.checks))

def test_dag_baseline_half_adder_datapack_api():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_raw_half_adder_datapack(out)
        funcs = out / 'data/ro_half_base/function'
        tests = (funcs / 'tests.mcfunction').read_text()
        assert 'function ro_half_base:build' in tests
        assert 'case_00' in tests and 'case_11' in tests
        case = (funcs / '_case_01.mcfunction').read_text()
        stim = (funcs / '_stimulate_01.mcfunction').read_text()
        check = (funcs / '_check_01.mcfunction').read_text()
        assert 'stimulate_01 4t replace' in case
        assert 'minecraft:redstone_block' in stim
        assert 'check_01 60t replace' in stim
        assert 'expected SUM=1 CARRY=0' in check
        build = (funcs / '_build.mcfunction').read_text()
        assert 'replace minecraft:redstone_wire' in build
        assert 'fill ' in build

def test_semantics_connectivity_probe_catalog():
    cfg = JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20)
    probes = {p.name: p for p in semantic_probes(cfg)}
    for name in ('10_dust_repeater_dust', '11_repeater_reverse_blocked', '12_dust_to_repeater_input', '13_dust_corner', '14_dust_stair_up', '15_dust_stair_down', '16_leaf_dust_block_power', '17_branch_leaf_block_power', '18_repeater_route_block_power', '19_cell_output_to_block_input', '20_repeater_to_corner'):
        assert name in probes
        probes[name].world.validate_supports()

def test_semantics_automatic_suite_reuses_one_origin():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_semantics_datapack(out, config=JavaExportConfig(namespace='ro_sem', pack_format=71, test_delay_ticks=20), build_settle_ticks=4)
        funcs = out / 'data/ro_sem/function'
        tests = (funcs / 'tests.mcfunction').read_text()
        assert 'same-origin' in tests
        for n in ('01_source_to_dust', '14_dust_stair_up', '20_repeater_to_corner'):
            launch = (funcs / f'_launch_{n}.mcfunction').read_text()
            assert 'tag=ro_sem_suite_origin' in launch
            assert 'positioned ~' not in launch
        gallery = (funcs / 'gallery.mcfunction').read_text()
        assert 'positioned ~247 ~ ~' in gallery
        assert 'positioned ~368 ~ ~' in gallery

def test_multi_circuit_datapack_api():
    from pathlib import Path
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / 'pack'
        export_multi_circuit_datapack(out)
        funcs = out / 'data/ro_circuits/function'
        assert (funcs / 'mux/tests.mcfunction').exists()
        assert (funcs / 'decoder/tests.mcfunction').exists()
        mux = (funcs / 'mux/tests.mcfunction').read_text()
        dec = (funcs / 'decoder/tests.mcfunction').read_text()
        assert 'case_000' in mux and 'case_111' in mux
        assert 'case_00' in dec and 'case_11' in dec
