# Structured Blueprint review diagnostics

Generation errors and Blueprint review/adoption responses share
`dustroute.review-diagnostics.v1`. Read `diagnostics.status` first: `failed`
means a declared obligation has a counterexample; `undetermined` means the
review did not establish it. Neither can authorize adoption.

`findings` identifies the occurrence and pinned revision when available, check
kind, status and human-readable detail. `evidence` carries the available facts:
type revision, terminal, position, expected/actual block, initial or committed
runtime observation, game tick/section, and observed lever values. An unknown
input is `null`, never implicitly OFF. The first retained contradiction can be
on extension or retraction; its observed input values describe that state.

Behavior evidence retains the verifier's report as structured JSON, including
available counterexamples, input prefix/cycle and exploration budget/closure.
Report shapes follow the selected verifier. Do not parse the detail string or
assume every check provides a complete execution trace. No remedy is invented
when the evidence only identifies an unknown or insufficient observation.

Public responses include at most 64 findings, prioritizing failures before
undetermined checks. `total_findings`, `failed_checks`, `undetermined_checks`
and `omitted_findings` count the complete diagnostic view before truncation.
The full review remains in historical proposal events. Saving/reloading that
history does not restore an executable runtime or grant adoption authority;
adoption still performs fresh review.

For building failures, inspect `errors[].code`, `item`, `position`, and
`diagnostics.findings`. Geometry errors identify the offending item/coordinate;
whole-Assembly failures use `verification_not_established` with this shared
diagnostic view. The enclosure, explicit-design and door-composition generators
use the same error type. Failed generation publishes no records and performs
no world writes. `live_world_verified` remains false.

Offline regression coverage includes moving child failures, behavioral
counterexamples, zero-step budgets and bounded omissions, saved-review round
trips, restart/adoption refusal, and failed public composition without catalog
mutation. Transport stubs establish API behavior, not live Minecraft parity.
