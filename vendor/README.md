# Pinned Voxrig source

`voxrig/` is an unmodified source snapshot from the separate Voxrig repository.
`voxrig-source.json` records its full commit, included paths and every file's
SHA-256. The source, registry tables, native oracle fixtures, examples and their
licenses are included so a DustRoute checkout builds without an unpublished
sibling checkout. Cargo excludes this crate from the DustRoute workspace; it is
an optional path dependency selected by `--features voxrig`.

The current snapshot is Voxrig commit
`784c12dba126f5829cd7a8db8542360cc48434c9`, published on
[`codex/dustroute-integration`](https://github.com/sugipamo/Voxrig/tree/codex/dustroute-integration).
All 205 included files match that immutable commit, including the shared native
observation implementation. The [pin alignment verification](../docs/evidence/voxrig/source-pin-alignment-20261002.json)
records the standalone build checks.

Verify the recorded snapshot without network access:

```sh
python3 scripts/vendor_voxrig.py --check
```

Keep Voxrig changes in its own repository and commit them there first, preserving
the upstream contribution history. After validating that commit, update the
snapshot with:

```sh
python3 scripts/vendor_voxrig.py --source /path/to/Voxrig --revision FULL_COMMIT_SHA
python3 scripts/vendor_voxrig.py --check
cargo check --locked -p dustroute-mcp --features voxrig --all-targets
```

The updater refuses modified managed files and never deletes unknown files.
Do not edit `vendor/voxrig` directly. This snapshot can be replaced by a published
immutable upstream revision later without changing the version-adapter boundary.
The separate local Voxrig branch remains ready for upstream PR preparation;
vendoring does not publish it or create a PR.
