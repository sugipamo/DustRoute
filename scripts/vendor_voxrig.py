#!/usr/bin/env python3
"""Verify or update the exact Voxrig source snapshot used by this workspace.

Consumers: --check (no sibling checkout or network required).
Maintainers: --source /path/to/Voxrig --revision <full tested commit SHA>.
Existing managed files must be unmodified before an update; unknown files are
never deleted. The separate source repository remains the upstream PR history.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "vendor/voxrig"
MANIFEST = ROOT / "vendor/voxrig-source.json"
PATHS = ["Cargo.toml", "Cargo.lock", "LICENSE", "README.md", "THIRD_PARTY_NOTICES.md",
         "CONTRIBUTING.md", ".gitignore", "src", "data", "docs", "examples", "tests", "scripts"]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true")
parser.add_argument("--source", type=Path)
parser.add_argument("--revision")
args = parser.parse_args()

def digest(data):
    return hashlib.sha256(data).hexdigest()

def safe_path(name):
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or not path.parts:
        raise SystemExit(f"invalid vendor path: {name}")
    target = DEST.joinpath(*path.parts)
    for ancestor in (target, *target.parents):
        if ancestor == ROOT:
            break
        if ancestor.is_symlink():
            raise SystemExit(f"vendor symlink refused: {ancestor}")
    return target

old = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else None
if old:
    for name, expected in old["files"].items():
        target = safe_path(name)
        if not target.is_file() or digest(target.read_bytes()) != expected:
            raise SystemExit(f"modified/missing vendored file; preserve or revert before updating: {target}")
    for directory, children, names in os.walk(DEST):
        if Path(directory) == DEST:
            children[:] = [name for name in children if name not in ("target", ".local")]
        for name in children:
            if (Path(directory) / name).is_symlink():
                raise SystemExit(f"vendor directory symlink refused: {Path(directory) / name}")
        for name in names:
            relative = (Path(directory) / name).relative_to(DEST).as_posix()
            if relative not in old["files"]:
                raise SystemExit(f"unmanaged vendor input; preserve outside the snapshot: {relative}")
if args.check and not args.source:
    if not old:
        raise SystemExit("vendor manifest missing")
    print(f"verified {len(old['files'])} Voxrig files at {old['revision']}")
    raise SystemExit(0)
if not args.source or not args.revision or not re.fullmatch(r"[0-9a-f]{40}", args.revision):
    parser.error("updates require --source and a full 40-character --revision")
revision = subprocess.check_output(["git", "-C", str(args.source), "rev-parse",
                                   f"{args.revision}^{{commit}}"], text=True).strip()
if revision != args.revision:
    raise SystemExit("revision did not resolve to the requested commit")
archive = subprocess.check_output(["git", "-C", str(args.source), "archive", revision, "--", *PATHS])
files = {}
with tarfile.open(fileobj=io.BytesIO(archive)) as tree:
    for member in tree:
        if member.isdir():
            continue
        if not member.isfile():
            raise SystemExit(f"unexpected non-file source entry: {member.name}")
        safe_path(member.name)
        files[member.name] = tree.extractfile(member).read()
manifest = {
    "repository": "https://github.com/sugipamo/Voxrig", "revision": revision,
    "license": "MIT; retain THIRD_PARTY_NOTICES.md for generated data and fixtures",
    "projection": PATHS, "files": {name: digest(data) for name, data in sorted(files.items())},
}
if args.check:
    if manifest != old:
        raise SystemExit("vendored snapshot differs from requested source commit")
else:
    for name in files:
        target = safe_path(name)
        if target.exists() and (not old or name not in old["files"]):
            raise SystemExit(f"unmanaged file would be overwritten: {target}")
    for name, data in files.items():
        target = safe_path(name)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    for name in (old or {}).get("files", {}).keys() - files.keys():
        safe_path(name).unlink()
    MANIFEST.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
print(f"{'verified' if args.check else 'vendored'} {len(files)} files at {revision}")
