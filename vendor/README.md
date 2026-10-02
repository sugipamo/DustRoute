# Pinned Voxrig source

`voxrig/` is an unmodified source snapshot from the separate Voxrig repository.
`voxrig-source.json` records its full commit, included paths and every file's
SHA-256. The source, registry tables, native oracle fixtures, examples and their
licenses are included so a DustRoute checkout builds without an unpublished
sibling checkout. Cargo excludes this crate from the DustRoute workspace; it is
an optional path dependency selected by `--features voxrig`.

The current snapshot is Voxrig commit
`0d3c3793084f43044b134330f92c781ff5463251`, published on
[`codex/survival-construction`](https://github.com/sugipamo/Voxrig/tree/codex/survival-construction).
All 230 included files match that immutable commit, including native
observation, survival inventory swaps, stationary context, guarded outbound
frames, closed operation history and bounded mining observations. The
[sender/mining implementation validation](../docs/evidence/survival-mining-implementation-20261002.json)
records current source and integration checks, with continuation unvalidated.
The earlier
[mining comparison validation](../docs/evidence/survival-mining-comparison-20261002.json)
records the preceding comparison-only pin. The
[survival foundation validation](../docs/evidence/survival-foundation-20261002.json)
and the earlier
[pin alignment verification](../docs/evidence/voxrig/source-pin-alignment-20261002.json)
remains historical evidence for the previous pin.

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
