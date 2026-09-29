# Pinned Voxrig source

`voxrig/` is an unmodified source snapshot from the separate Voxrig repository.
`voxrig-source.json` records its full commit, included paths and every file's
SHA-256. The source, registry tables, native oracle fixtures, examples and their
licenses are included so a DustRoute checkout builds without an unpublished
sibling checkout. Cargo excludes this crate from the DustRoute workspace; it is
an optional path dependency selected by `--features voxrig`.

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
