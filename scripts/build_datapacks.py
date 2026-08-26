"""
Regenerate every in-game regression Data Pack in datapacks/.

Usage:
    python3 -m scripts.build_datapacks

Produces:
    datapacks/semantics_01_20.zip   /function ro_sem:tests
    datapacks/half_adder.zip        /function ro_half_base:tests
    datapacks/mux_decoder.zip       /function ro_circuits:mux|decoder/tests
    datapacks/half_subtractor.zip   /function ro_halfsub:halfsub/tests
"""

from __future__ import annotations

import shutil
import tempfile
import zipfile
from pathlib import Path

from dustroute.minecraft_semantics import export_semantics_datapack
from dustroute.minecraft_export import JavaExportConfig
from dustroute.raw_half_adder_export import export_raw_half_adder_datapack
from dustroute.minecraft_circuits import (
    export_half_subtractor_datapack,
    export_multi_circuit_datapack,
)

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "datapacks"


def _zip(src_dir: Path, dest: Path) -> None:
    if dest.exists():
        dest.unlink()
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(src_dir.rglob("*")):
            if path.is_file():
                zf.write(path, path.relative_to(src_dir).as_posix())


def main() -> None:
    OUT.mkdir(exist_ok=True)
    jobs = [
        ("semantics_01_20.zip", export_semantics_datapack, {}),
        ("half_adder.zip", export_raw_half_adder_datapack, {}),
        ("mux_decoder.zip", export_multi_circuit_datapack, {}),
        ("half_subtractor.zip", export_half_subtractor_datapack, {}),
    ]
    for name, exporter, kwargs in jobs:
        with tempfile.TemporaryDirectory() as tmp:
            build = Path(tmp) / "pack"
            exporter(build, **kwargs)
            _zip(build, OUT / name)
        print(f"built {name}")


if __name__ == "__main__":
    main()
