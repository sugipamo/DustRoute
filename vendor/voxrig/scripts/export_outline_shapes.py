#!/usr/bin/env python3
"""Regenerate audited outlines with a locally obtained Java 1.21.11 development JAR.

Input classpath must include TinyRemapper 0.14.0, mapping-io 0.8.0, ASM 9.10.1,
and the Minecraft Java 1.21.11 libraries. No Minecraft JAR is downloaded or shipped.
"""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--minecraft-jar", required=True, type=Path)
parser.add_argument("--mappings", required=True, type=Path)
parser.add_argument("--classpath-file", required=True, type=Path)
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
work = ROOT / ".local/outline-export"
work.mkdir(parents=True, exist_ok=True)
cp = args.classpath_file.read_text().strip()
native = args.minecraft_jar.resolve()
mappings = args.mappings.resolve()

def run(*command):
    subprocess.run(command, cwd=work, check=True)

java = ("java", "-XX:ActiveProcessorCount=1", "-Xmx1024M")
run(*java, "-cp", cp, "net.fabricmc.tinyremapper.Main", str(native), str(work / "remapped.jar"),
    str(mappings), "official", "named", "--fixpackageaccess", "--threads=1")
run("javac", "-proc:none", "-J-XX:ActiveProcessorCount=1", "-cp", cp, "-d", str(work),
    str(ROOT / "scripts/PrepareOutlineOracle.java"))
run(*java, "-cp", f"{work}:{cp}", "PrepareOutlineOracle", str(work / "remapped.jar"), str(work / "oracle.jar"))
oracle_cp = f"{work}:{work / 'oracle.jar'}:{cp}"
run("javac", "-proc:none", "-J-XX:ActiveProcessorCount=1", "-cp", oracle_cp, "-d", str(work),
    str(ROOT / "scripts/ExportOutlineShapes.java"))
names = ["outline_shapes", "outline_coverage", "outline_raycast_cases", "outline_rotation_cases"]
run(*java, "-cp", oracle_cp, "ExportOutlineShapes", *(str(work / f"{name}.json") for name in names))
outputs = {}
for name in names:
    # Stable map key ordering, float representation and gzip header across invocations.
    value = json.loads((work / f"{name}.json").read_text())
    data = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()
    suffix = ".json"
    if name.endswith("cases"):
        data = gzip.compress(data, mtime=0)
        suffix += ".gz"
    outputs[name + suffix] = data
manifest = {
    "minecraft": "Java 1.21.11", "mappings": "Yarn 1.21.11+build.6 (official -> named)",
    "minecraft_merged_sha256": hashlib.sha256(native.read_bytes()).hexdigest(),
    "mappings_sha256": hashlib.sha256(mappings.read_bytes()).hexdigest(),
    "transformation": "Package remap and access-flag widening only; native method bodies unchanged. Audited state-only outline and auxiliary raycast boxes. Native BlockView.raycast OUTLINE/NONE with absent ShapeContext. Entity.getRotationVector on an unspawned armor stand, no world simulation.",
    "generator": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in
        ["scripts/ExportOutlineShapes.java", "scripts/PrepareOutlineOracle.java", "scripts/export_outline_shapes.py"]},
    "outputs": {p: hashlib.sha256(data).hexdigest() for p, data in outputs.items()},
}
outputs["outline_source.json"] = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
for name, data in outputs.items():
    target = ROOT / "data/java_1_21_11" / name
    if args.check:
        if target.read_bytes() != data:
            raise SystemExit(f"generated outline data differs: {target}")
    else:
        target.write_bytes(data)
print("outline data verified" if args.check else "outline data generated")
