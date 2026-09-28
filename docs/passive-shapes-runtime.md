# Passive shapes, support faces and conduction

The Java 1.21.11 callback runtime resolves passive geometry from a checked Rust
registry and the exact observed block state. A full support face and electrical
conduction are separate facts. The same resolution feeds electrical queries,
attachment lifetime, construction validation and literal block export.

## Admitted states

| Native identities | State | Full support faces | Conducts redstone |
| --- | --- | --- | --- |
| `stone_slab`, `smooth_stone_slab` | `type=bottom,waterlogged=false` | Down | No |
| Same two slabs | `type=top,waterlogged=false` | Up | No |
| Same two slabs | `type=double,waterlogged=false` | All six | Yes |
| `glass`, `tinted_glass`, sixteen explicitly listed stained-glass colors | No properties | All six | No |
| Existing `stone`, `cobblestone`, `smooth_stone`, `obsidian`, `smooth_quartz`, `cyan_wool` | No properties | All six | Yes |

`physical::passive::BUILTINS` declares names and either fixed geometry or three
dry slab states. Its const validator rejects duplicate names, non-passive kinds,
incorrect slab faces, attachment requirements and device signal axes. Physical
descriptors also reject conducting partial shapes. New identities with these
same semantics can be added as Rust constants after verifying their native
behavior; arbitrary shapes or lifecycle behavior still need an explicit primitive.

Slabs retain the existing `Transparent` observation kind for all three states.
The state selects the physical descriptor; kind alone cannot decide whether a
double slab conducts. Exact admission requires both slab properties and rejects
waterlogged, incomplete, extra or invalid properties, coarse evidence and foreign
namespaces. Suffix-based observation of other slabs is not execution admission.

Top slabs support floor components and a wire step, but lack a full vertical
side face: the lower wire's rendered connection is `side`, not `up`. Double slabs
and glass have a full vertical side face. Dust signal propagation independently
uses conductor and clearance queries. Bottom slabs support ceiling-mounted
levers/buttons, but cannot support floor components at the next block position.
These geometries have identical FULL/RIGID support results on their admitted
full faces; this is not a general voxel-shape implementation.

Piston motion uses the existing payload mechanism and preserves the native name
and both slab properties. Losing a slab's support invokes the shared attachment
removal callbacks. Snapshot/Assembly serialization, generic Java export and
electrical construction preserve literal material and state rather than replacing
an observed slab with the configured synthetic glass material. Synthetic block
templates still use export configuration. Existing placement validation rejects
malformed admitted passive states.

Execution and root exploration advance to **v15**, physical admission to **v6**.
Device programs remain v7, synchronous runtime v5 and root comparison v3.
Saved v14 contexts require fresh review from explicit initial conditions. No
checkpoint conversion or reinterpretation of published finite spatial Laws is
introduced. Historical finite-law diagnostics retain their own geometry contract.

## Target reference and independent trials

The cached mapped Java 1.21.11 / Yarn build.6 artifact supplies the static
reference. `SlabBlock.getOutlineShape` selects top, bottom or full cube;
`AbstractBlock.Settings` installs `method_26248` (`isFullCube`) as its default
solid-block predicate. Stone and smooth-stone slabs retain that default.
`Blocks` explicitly installs `never` for glass, tinted glass and the shared
stained-glass registration helper, independently of opacity. Slab neighbor
updates schedule fluid work only for waterlogged states, which are excluded.
`RedstoneWireBlock` and `SideShapeType` supply the existing slope and support
queries. Class hashes and audited methods are in the evidence manifest.

`tools/make_passive_shape_fixtures.py` declares ten small circuits without model
expectations: five conductor tests, three bidirectional wire-step tests and two
moving-support tests. Ordinary player inputs run against the existing private
instrumented server with only `minecraft:vanilla` enabled. One rotated moving
support loses a pressed ceiling button; the other loses dust on a top slab.
Independent marker pistons in conductor/step trials also exercise restoration
while a carrier exists, including nonconducting cases.

The retained evidence includes actual input application ticks, contiguous raw
intervals, complete bounded tick-end palettes, ordered palette writes, scheduled
device deliveries and discarded attempts, and available device/piston callback
boundaries. Replays verify checkpoint and behavior-state continuation. This does
not reconstruct hidden state from live worlds or directly observe every shape
callback. Each completed trial requires empty-region cleanup, owned force-load
removal and normal server shutdown.

## Measured result

Ten independent captures on 2026-09-28 match the declared projection. All use
Y=180, Z=1000; input timing is derived from server writes. Counts exclude
construction and cleanup. The [evidence manifest](evidence/passive-shapes-20260928.json)
records actual inputs, original and final model hashes, source classes, retained
raw intervals and validation commands.

| Run | Trial | Rotation / X | Tick ends | Writes | Delivered ticks | Callbacks |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| a | Bottom stone slab conduction | 0 / 91000 | 58 | 10 | 2 | 35 |
| b | Top stone slab conduction | 0 / 92000 | 56 | 10 | 2 | 35 |
| c | Double stone slab conduction | 0 / 93000 | 57 | 14 | 3 | 55 |
| d | Tinted glass conduction | 90 / -94000 | 57 | 10 | 2 | 35 |
| e | Red stained glass conduction | 0 / 95000 | 57 | 10 | 2 | 35 |
| f | Top stone slab wire step | 0 / 96000 | 89 | 16 | 0 | 83 |
| g | Double smooth-stone slab wire step | 90 / -97000 | 89 | 44 | 0 | 307 |
| h | Tinted glass wire step | 0 / 98000 | 89 | 16 | 0 | 83 |
| i | Moving top stone slab, dust detaches | 0 / 99000 | 82 | 19 | 3 | 64 |
| j | Moving bottom smooth-stone slab, ceiling button detaches | 90 / -100000 | 85 | 20 | 6 | 60 |

Across the captures, 719 tick ends, 169 writes, 20 delivered ticks, 21 scheduler
attempts and 792 callback boundaries match. Run j includes a discarded release
attempt after the pressed button has disappeared. Run c powers the downstream
lamp and dust; the other four conductor trials do not. In f/h only the lower
wire powers the upper wire; g propagates in both directions. The coverage test
requires these events, including exact slab state after motion, so an inactive
circuit cannot satisfy the evidence check. All ten trials cleaned the region,
removed owned force loads and stopped normally. No instrumentation source was
changed; existing uncommitted probe work and its hashes are recorded explicitly.

## Validation

- 367 Rust tests passed: the complete Minecraft crate (280), library Law/context
  checks (18), affected translation/construction/adoption targets (42), export
  and snapshot unit tests (12), and public MCP blueprint tests using a transport
  stub (15).
- All 26 compile-fail doctests and 27 Python tests passed. The latter independently
  rederive and replay all 31 retained device circuits, plus the existing reference
  door and transient observations, using the rebuilt model.
- New Rust cases cover every admitted slab state and glass identity, all support
  directions, malformed/coarse/wet evidence rejection, piston push/pull state
  preservation, Assembly save/reload and support-first construction/teardown.
- One old wire-rise fixture omitted the slab's `waterlogged` property. It now
  explicitly declares `false`; incomplete evidence remains rejected. The initial
  failing run is retained alongside the successful final checks.
- Workspace/all-target Clippy with warnings denied, formatting and diff checks
  passed. Cargo used one job and Rust tests one thread.

Validation is scoped to the affected paths; a complete workspace test run or a
new live reference-door trial is not claimed.

## Scope

This extension covers the identities and dry states listed above. Other slab
materials, stairs and their neighbor-dependent shape changes, panes, water,
entities, drops, inventories, slime/honey and direct piston crushing remain
outside scope. There is no new public MCP operation or operational companion
MOD. The live trials start from strict fresh placement; they do not certify
arbitrary construction histories or a new live 3×3 door variant.

The standing stop condition applies before implementing an outside-scope
prerequisite or an unagreed substantial redesign. Cargo uses offline locked
dependencies, one job and one Rust test thread. No host changes or fault
injection are part of this work.
