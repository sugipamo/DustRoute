# Attachment support loss

The current Java 1.21.11 callback runtime detaches admitted components when
their required support disappears or changes shape. This follows the target's
neighbor/shape callbacks, including removal caused by piston motion or explicit
support removal. It does not sweep the world after every write.

## Shared rules

`physical::PhysicalSpec::support_loss` is a checked Rust constant declaration.
It declares the triggering callback, the required face and gate-specific
post-removal neighbor notifications. Invalid attachment/trigger/orientation
combinations fail const validation; dynamic deserialization uses the same checks.

| Component | Trigger | Required support |
| --- | --- | --- |
| Dust | Support-side shape or ordinary neighbor | Full top face |
| Standing redstone torch | Support-side shape | Center of the top face |
| Wall redstone torch | Support-side shape | Full attached face |
| Lever / stone button | Support-side shape | Full attached face |
| Repeater / circuit comparator | Support-side shape or ordinary neighbor | Rigid top face |

Full and rigid requirements coincide for the currently admitted geometry.
Extended piston bodies retain full support on the back face and centered
support on the sides. Head fronts are full; front/back rod centers support
standing torches. A registered retracting source retains its stationary base
shape. An unregistered moving carrier provides no such support.

Automatic removal shares the existing removal-notification queries and uses a
single Air delta. The core clears block-owned state, including comparator output.
Removed-block notifications, wire recalculation, ordinary neighbors and shape
updates follow native order; nested neighbor work drains after the originating
updater entry. Gate removal adds notifications around each of six neighbors.
Queued ticks retain their concrete receiver guard, so a removed button's release
attempt cannot recreate it. No dropped-item entity is simulated.

Between a support write and its callback, an attachment can temporarily lack
support. Checkpoints preserve that continuation and restore it exactly. Fresh
initial worlds still require valid support; restoring a queued intermediate
world does not authorize unsupported fresh initialization.

Execution/root exploration advances to **v14**, physical admission to **v5**.
Device programs remain v7, the synchronous record v5 and root comparison v3.
Earlier execution contexts/checkpoints/approvals are not reinterpreted. Start a
fresh review from explicit initial conditions; there is no checkpoint conversion.
Historical finite spatial Law bodies remain unchanged. Current placement support
diagnostics and runtime installation share the directional attachment rule.

## Reference and observation boundary

Source inspection uses the cached mapped Java 1.21.11 / Yarn build.6 classes:
WallMountedBlock, AbstractTorchBlock, TorchBlock, WallTorchBlock,
AbstractRedstoneGateBlock, RepeaterBlock, ComparatorBlock, RedstoneWireBlock, PistonExtensionBlock,
PistonHeadBlock, SideShapeType, Block, World, WorldChunk and ServerWorld.
The independent live trials use the existing private instrumentation and ordinary
player inputs, with only `minecraft:vanilla` enabled.

The support fixtures move a three-block stone line underneath an attachment,
with an observer/lamp reading its disappearance. Powered dust, powered gates,
a pressed button, standing/wall torches and powered levers exercise removal
effects. Separate piston-body fixtures distinguish dust from centered torch
support. Fixtures declare initial conditions, not expected model output.

The probe records scheduler attempts before `ServerWorld.tickBlock` checks the
current block identity. Support fixtures therefore compare every attempt and
its delivery/discard decision separately from delivered ticks. A discarded
button release remains evidence even though it writes no block. Existing
retained fixture projections keep their published meaning.

Comparisons cover complete bounded tick-end palettes, ordered palette commits,
device tick delivery/attempt order, active-device ordinary notifications and
piston event/carrier boundaries. Input time comes from the actual server write,
not the client's requested wait. Hidden registers, histories, shape callbacks,
inert-target callbacks and item entities are not directly observed.

The [evidence manifest](evidence/support-loss-20260928.json) records retained raw
intervals, original artifact hashes, actual inputs, target/probe sources and
validation results. Local full captures use
`.local/e2e-artifacts/support-loss-20260928-<run>`. Every completed trial requires
empty-region cleanup, removal of owned force loads and normal server shutdown.

## Measured result

Eleven independent captures on 2026-09-28 match the final v14 model's declared
projection, including checkpoint and behavior-state continuation. All origins
use Y=180, Z=1000. Counts cover the input/drain interval, excluding construction
and cleanup. Every trial cleaned the region and stopped the private server.

| Run | Attachment / operation | Rotation / X | Tick ends | Writes | Delivered ticks | Callbacks |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| a | Dust / pushed support | 0 / 80000 | 81 | 19 | 3 | 64 |
| b | Pressed button / pushed support | 0 / 81000 | 85 | 24 | 6 | 69 |
| c | Powered dust / pushed support | 0 / 82000 | 81 | 19 | 3 | 64 |
| d | Standing torch / pushed support | 0 / 83000 | 80 | 19 | 3 | 64 |
| e | Wall torch / pushed support | 90 / -84000 | 81 | 19 | 3 | 64 |
| f | Powered lever / pushed support | 0 / 85000 | 81 | 19 | 3 | 64 |
| g | Powered repeater / pushed support | 0 / 86000 | 85 | 22 | 4 | 67 |
| h | Powered comparator / pushed support | 0 / 87000 | 86 | 22 | 4 | 67 |
| i | Dust / source body extends and retracts | 0 / 88000 | 81 | 9 | 0 | 33 |
| j | Standing torch / source body extends and retracts | 0 / 89000 | 81 | 8 | 0 | 32 |
| k | Dust / sticky piston pulls support away | 90 / -90000 | 41 | 12 | 3 | 28 |

In b, the seventh scheduler attempt is the removed button's discarded release.
In j, the torch survives the complete body cycle; in i, the dust is removed.
In a–h, returning solid support does not recreate a detached component. The
fixture coverage test requires these observations, powered gate removal and
observer/lamp responses; it does not accept a replay that never exercises them.

The initial g/h comparisons found correct final blocks but late gate removal.
The concrete RepeaterBlock and ComparatorBlock shape callbacks both reject a
missing DOWN support before ordinary neighbor delivery. Adding that trigger to
the shared gate declaration fixes ordered writes and notifications. Original
mismatching comparisons remain unchanged; separately named final comparisons
match the same raw captures. No per-fixture physics exception was introduced.

Across these captures, 863 tick ends, 192 palette writes, 32 delivered device
ticks, 33 scheduler attempts and 616 callback boundaries match.

## Validation

- 349 Rust tests passed: the complete Minecraft crate (276), library Law/context
  checks (18), affected translation/construction/adoption targets (40) and public
  MCP blueprint tests with a transport stub (15).
- All 24 compile-fail doctests and 26 Python tests passed. The latter rederive
  and replay all retained device captures, earlier 3×3 door observations and
  transient comparisons using the rebuilt model.
- Source-derived tests cover 20 attachment mounts, restoration while support is
  absent but removal is pending, a discarded button-release tick, comparator
  register clearing and sticky retraction. Admission rejects unsupported fresh
  worlds and incompatible saved v13 contexts.
- Workspace/all-target Clippy with warnings denied, formatting and diff checks
  passed. Cargo used one job and Rust tests one thread. The final validation is
  scoped; no complete workspace test run is claimed.

## Scope

This implements attachment lifetime for existing admitted blocks. It does not
add new materials/shapes, pressure-plate/entity behavior, item transport or
collection, piston crushing of a component directly in the movement path,
slime/honey, inventories or fluids. These trials start from strict fresh
placement; they do not certify arbitrary live construction or reconstruct hidden
live state. There is no new public MCP operation or operational companion MOD.

Stop and report before implementing an outside-scope prerequisite. Builds use
offline locked dependencies, one Cargo job and one Rust test thread. There are
no host operations or fault-injection trials.
