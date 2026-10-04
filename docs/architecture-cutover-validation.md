# Cutover verification and remaining format retirement

This follows the [architecture cutover](architecture-cutover.md). Verify the
additional consumers and retained physical comparisons before removing remaining
storage compatibility. Keep current-format reload, fresh adoption and explicit
rejection of retired data. No new Minecraft feature is required.

The later [live operation checks](architecture-cutover-live-validation.md) cover
fresh adoption, restart, construction, operation, repair and removal on the
private Java server. They also identify and fix a bounded-context validation
defect in upward wire repair; those results are separate from the offline pass
reported below.

## Inventory and scope

| Boundary | Finding | Action |
| --- | --- | --- |
| Blueprint catalog, placed instances, repair records, mutation RPC and translation root API | Previous cutover removed the old readers and forwarding paths | Retain rejection tests; they do not implement compatibility |
| Blueprint update archive | Writer previously chose v1–v5 from the contents | Removed minimum-version detection and v1–v4 readers; always write/read v5 |
| Player-scoped MCP Blueprint store | v1 allowed an absent grounding map and substituted an empty map | Retired v1; v2 uses a typed record with a required grounding map, including explicit empty maps |
| Recorded Minecraft traces and definition IDs | Older captures are comparison evidence; `*.v1` definition IDs are immutable domain identities | Preserve original evidence and pins; file names alone do not identify an obsolete archive format |
| Optional modeled Assembly in circuit revisions and placement reports | Current observations may contain undecodable block states; a literal snapshot can exist without a modeled Assembly | Retain this representable state; removing it requires a separate observation-contract change |
| Fixed-geometry, compatibility and moving-world execution | They carry different proof contracts and still have current callers | Retain these execution models; archive cleanup does not replace their physics |

The optional fields in capture/scenario records also describe incomplete
observations. Removing them or rewriting old measurements would change the
available evidence, so they are outside this storage-format retirement.

## Verification status

Before removing the additional compatibility paths:

- App, physical, IR and CLI: 52 passed.
- All 47 translation integration suites omitted from the previous pass: 206
  passed, no failures. One pre-existing manual default-budget reachability
  measurement remains ignored; it does not certify behavior.

This includes recorded native callbacks, scheduler observations, mixed electrical
pistons, support/shape behavior, doors and generated flying machines. Original
capture records are preserved. After removing the compatibility paths:

- Six translation adoption/restart suites: 48 passed.
- Full MCP library: 100 passed, including public adoption after restart,
  construction/removal/reconstruction, grounding capture and refusal to rewrite
  retired archives on either read or write requests.

There were no failures. The two phases ran 406 passing checks in total; that
count includes rerunning affected cases after the format change. Formatting and
workspace/all-target Clippy with warnings denied passed, as did diff whitespace
checks. No scope-expanding prerequisite or overall blocker was found.

## Retirement behavior

Blueprint update v5 is used even for an empty proposal history or one without a
behavior context. Reload still reconstructs the catalog, checks proposal ancestry
and decisions, and reruns required verification at adoption. No saved pass becomes
permission through a format change.
The content-dependent version detector is removed. The shared
`BehaviorReviewContext::is_runtime` predicate remains because current promotion
reports also use it; it is not an archive compatibility path.

MCP store v2 requires the owner, nested update archive and explicit `groundings`
map. Old outer v1, nested update v1–v4 and missing grounding maps fail loading.
Both reads and writes refuse these files without replacing their bytes. Files
are not silently upgraded, deleted or reset to an empty catalog. Preserve/move
old state outside the active store and recreate/freshly review it as described in
the [cutover guide](architecture-cutover.md); do not relabel old headers as proof.

Rust commands run one at a time with `--offline --locked -j1` and
`--test-threads=1`. Retained observations are replayed offline; this is not a new
live Minecraft trial. Existing user state files are not deleted by these checks.

## Reproducing the checks

```sh
cargo test --offline --locked -j1 -p dustroute-app -p dustroute-physical -p dustroute-ir -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --tests -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-translate --test blueprint_updates --test runtime_adoption --test ordinary_reference_door --test physical_periodic --test repeated_settling_adoption --test flying_machine_adoption -- --test-threads=1
cargo test --offline --locked -j1 -p dustroute-mcp --lib -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --offline --locked -j1 --workspace --all-targets -- -D warnings
```

The `--tests` command reproduces the combined translation coverage of this and
the preceding pass. During this follow-up, the 47 previously omitted integration
suites were selected explicitly; the library and 13 integration suites already
had passing results. After retiring the formats, the six affected adoption
suites were rerun. JavaScript and Python were unchanged and retain the checks
recorded in the preceding cutover.
