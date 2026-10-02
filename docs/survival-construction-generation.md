# Automatic survival construction: implementation progress

Stage 2 of the [approved roadmap](survival-construction-roadmap.md) is complete
for its declared bounded acceptance cases. On 2026-10-02 the automatically
generated roof completed all 115 actions on the isolated non-OP server, removed
all 18 temporary blocks, retreated and matched 3,120 independently observed
cells. Public MCP integration and interrupted-job continuation remain later work.

## Generated live acceptance (Temurin recovery)

[Acceptance evidence](evidence/survival-generation-acceptance-20261002.json)
retains the generated plan, final journal, complete received trace windows,
server/test logs, runtime metadata, controller and hash-checking archiver.

- Source: `2c37c00` (executable code unchanged since `da9bdd0`); native source
  remains `2b6e7bfc94e6270054eac5c7b14a74d4657a411c`.
- Java: project-local Temurin `21.0.12.1+1`, archive SHA-256 checked against both
  official GitHub asset metadata and the published checksum. System Java unchanged.
  Normal tiered compilation, no C2 bypass; unchanged one-CPU/768-MiB heap limits.
- Actions: 49 permanent placements, 18 temporary placements, 30 moves and
  18 temporary removals. All 18 removal/reconnect boundaries verified.
- Result: journal `completed`/`observed`, zero temporary blocks, exact planned
  retreat position and independent final check of all 3,120 cells.
- Test: 236.30 seconds including fixture synchronization and planning. Entire
  controller run including server start/stop: 248.17 seconds. First-to-last
  completed actions: 161.52 seconds; this interval excludes the first action and
  final verification, so it is not total construction or pure placement time.
- Test and server both exited 0. No recurrence of the JVM crash in this trial;
  long planning and the full sequence also completed without keepalive timeout.

The user authorized this recovery trial and asked not to investigate further
if the crash did not recur. JVM root-cause investigation is therefore closed
without attribution. One successful run is not proof that every JDK/environment
issue is fixed. The earlier failure records below remain historical evidence.

## Shared checker and library boundary

`survival_construction::CheckedPrefix` is private and branchable. It validates
the initial capture and site once, extends one candidate action through the
same native geometry and ownership checks as authored sequences, and only
produces a `HypotheticalConstructionPlan` after exact final geometry, complete
temporary cleanup, supplied materials and safe retreat are checked.

The ordinary `preview_construction_sequence` uses this same implementation.
Speculative extensions consume a cloned predecessor; a refused branch cannot
modify its sibling. Successful output has the existing non-deserializable
hypothetical plan type, not native action authority. Actual execution still
requires fresh checks through `SurvivalExecutor`.

Voxrig remains pinned at `2b6e7bfc94e6270054eac5c7b14a74d4657a411c` throughout
these caller-planning changes. No native physics or operation contract was
weakened. Movement candidates use native scenario prediction and the shared
travel/terminal admission check. The existing round-trip route API is not used
as a proof of escape after future edits.

## Generator contract

`generation::generate_construction_plan` takes a captured scene, checked site,
proposed supplied-material budget, temporary material and explicit search limits.
Async clients use `generate_construction_plan_async`, which moves detached owned
data into a bounded CPU worker so native receive/keepalive tasks can continue.
Dropping its awaiter discards the result but does not abort a started read-only
worker. Neither entry point owns a live action handle.

It returns a complete hypothetical plan with counters, or structured invalid
input, unsupported target state, initial-scene refusal, material shortage or
bounded-search failure. The material budget is not an inventory receipt.

Limits cap candidate extensions (including refusals), expansions, retained
frontier and action count. A candidate extension can invoke several native
checks, so this counter is not described as a count of physics calls. Native
edit/tick limits remain unchanged. Intermediate successor sets are also bounded.
Failures identify budget/frontier exhaustion and retain bounded refusal/progress
samples and hypothetical remaining targets; they do not prove impossibility.

Reservations include unbuilt permanent targets. Temporary placement consumes
supplied material even when permanent work uses the same material. Removal does
not refund hypothetical drops. Only already owned temporary cells are removable.

## Current candidate policy

- Greedily place reachable permanent targets through native face and reach checks.
- Generate temporary placements inside the explicit allowed footprint. Evaluate
  columns together with native movement onto them; keep every ordinary placement
  and movement in the resulting sequence.
- Derive standing and overhanging work positions from observed/scenario surfaces.
  Sample bounded walk/jump controls and require native whole-body, support and
  terminal clearance. An overhang coordinate is a candidate, not permission.
- Retain ascent, descent and level alternatives; prefer work positions separated
  from the target and choose nearby unfinished work before comparing approaches.
- Prefer top-down temporary cleanup while retaining lower tiers until descent.
  Every removal still models the existing native reconnect initialization.
- Evaluate reachable permanent work after candidate moves. If access stops making
  useful progress, try a separately bounded cleanup subgoal before transferring
  to other work. Cleanup does not add temporary blocks. Its checks and expansions
  count against the global budgets; its second frontier is independently bounded.
- Apply bounded best-first selection. Geometric scores, greedy placement, limited
  position/control samples and non-retracing motion are incomplete search policy,
  not additional Minecraft physics or an exhaustive dependency analysis.

These policies may fail on a physically constructible design. They must not
return a partial branch as a completed plan or dispatch it to the executor.
No coordinate-specific roof operation list is used by the generator.

## Checks completed so far

The related Rust suite has 21 passing tests and four opt-in live fixtures.
MCP all-target Clippy passes with Voxrig enabled. Contract tests include reserving
permanent materials against temporary use, no drop refunds and invalid budgets.

On source `c67410a`, isolated vanilla captured-scene comparisons passed for:

| Design | Generated actions | Candidate checks |
| --- | ---: | ---: |
| Two-block column | 2 | 2 |
| Supported four-block beam | 7, including movement and retreat | 548 |
| Translated two-block column | 2 | 2 |

Each plan matched a fresh complete replay through the public sequence checker.
A refused native extension left the predecessor usable. Material shortage and
one-check exhaustion also returned their declared diagnostics. These were
read-only comparisons against actual received geometry: no generated building
actions were sent, and they do not substitute for full-roof live acceptance.

## Full generated roof preflight

On `c67410a`, the original roof passed complete hypothetical generation without
an authored operation list: **115 actions, 49 permanent placements, 18 temporary
placements, 30 moves and 18 removals**. Required supplied materials are 49
cobblestone and 18 dirt (no future drop credits). Two access cleanup phases
complete, no temporary blocks remain, and the final feet are approximately
`[0.541636, -60, -1.499938]` inside the ground retreat scope.

The search used 9,963 candidate checks, 50 expansions and a peak frontier of 16
under a 12,000-check limit. This remains an incomplete bounded heuristic, not a
guarantee for arbitrary roofs. The unchanged native checks admit all actions.
[Progress evidence](evidence/survival-generation-progress-20261002.json) includes
all prior failures and the complete preflight, with source pins and hashed logs.

## Live attempts and stop condition

[Live-attempt evidence](evidence/survival-generation-stop-20261002.json) preserves
these distinct outcomes rather than replacing them with the successful preflight:

- **Live A (`c67410a`):** both clients timed out during synchronous planning on
  the fixture's current-thread Tokio runtime. At step zero, an inventory swap
  returned an uncertain write; the executor recorded `needs_inspection` and did
  not replay it. No placement/mining/motion dispatch appears in native history.
- **Async correction (`da9bdd0`):** the generator's async entry point and fixture
  planning use `spawn_blocking`. Related tests: 21 passed, four opt-in tests
  ignored; all-target MCP Clippy passed. No Voxrig change was needed. Earlier
  long read-only preflights do not demonstrate connection liveness.
- **Live B:** fresh world startup exceeded the fixture's 60-second readiness
  timeout. The server subsequently became ready and stopped normally; no Rust
  test or building operations started. The fixture startup timeout alone was
  increased to 180 seconds for the next attempt.
- **Live C (`da9bdd0`):** the isolated Java server crashed with SIGSEGV on
  `C2 CompilerThread0`, in `PhaseChaitin::post_allocate_copy_removal()`. The JVM
  identifies itself as OpenJDK `21.0.12+8-1-26.04-Ubuntu`. Detached planning still
  produced the same 115-action result; fresh executor creation refused EOF.
  No executor journal was created and native history contains no construction
  dispatch. A bounded crash-summary excerpt is retained; the full report stays
  local because it includes environment data.

The JVM crash is an observed process failure, not a determination of its root
cause. All dependent live work stopped under the user's prerequisite stop rule.
No JDK replacement, JVM flag workaround or additional crash reproduction was
attempted. Long-running transport liveness after the async correction still
needs successful live validation.

## Acceptance that remained at the JVM stop (historical)

First agree on a narrowly scoped environment investigation: identify a supported
server JDK/build and decide whether replacement or a diagnostic JVM configuration
is appropriate. Then use a fresh isolated fixture to execute the generated roof
through the unchanged common executor, retaining exact final geometry, all
owned-temporary cleanup, independent observation and ground retreat evidence.

Stage 2 remains incomplete until that actual acceptance succeeds. Blueprint/MCP
integration, reobserved continuation and whole-workflow failure tests remain
subsequent milestones; the previously completed authored roof is separate proof.

## JVM investigation (2026-10-02, read-only investigation authorized)

The user authorized investigation after the stop. No Java process, server trial,
compiler replay, JDK installation, package update or JVM flag change was performed
in this investigation. No PVE host operation was performed. Source code and the
native pin are unchanged.

### Observations and limits of attribution

- The preserved report still hashes to
  `73b03af6c96d6c8621b50b4e226641aae8f02556010347c39f2f12145867ba26`.
  It records a crash at `2026-10-02 15:25:02 UTC`, 15.386 seconds after VM start,
  in `C2 CompilerThread0`, compiling `dwz::a` (506 bytecodes), compilation level 4.
  The stack goes through C2 code generation/register allocation to
  `PhaseChaitin::post_allocate_copy_removal()+0xa0b`. This identifies the failing
  phase, not the precise defective source line or the root cause.
- The server logged both clients joining and fixture preparation at 15:25:00.
  Live C's final client history has no placement, mining, motion or pending
  inventory swap. Its only recorded phase is successful detached plan generation;
  fresh executor admission refused `unexpected end of file`. There is no evidence
  that the generated construction sequence had begun dispatching.
- `/usr/bin/java` resolves to the installed Ubuntu Java 21 binary. Both package
  metadata and its release file agree with the crash: runtime
  `21.0.12+8-1-26.04-Ubuntu`, package `21.0.12+8-1~26.04`, x86_64.
  `dpkg --verify openjdk-21-jre-headless` exited 0 without discrepancies. This
  checks installed files covered by package metadata; it does not prove the
  distribution build correct, all dependencies intact, or runtime memory sound.
- The saved crash report has about 480 MiB RSS, zero process swap and a 768 MiB
  maximum Java heap. The terminal failure is SIGSEGV, not an out-of-memory
  exception. These facts do not rule out system pressure or native corruption,
  but provide no basis for prescribing a larger heap as the fix.
- The report's replay file exists (2,514,408 bytes, SHA-256
  `48449c913b343fc368d98f291f3804cb81b72ae0104263df63160cc7f6356ea3`).
  It was not executed or uploaded. No core dump was produced. The full report and
  replay stay local; inspect for sensitive data before any future upstream report.
- Searches of public OpenJDK/Java issue material did not establish a matching
  fixed bug. Ubuntu's Launchpad pages were inaccessible to the browsing tool.
  Absence of a match is not proof that no matching bug exists. A JVM compiler
  defect or build-specific problem is a reasonable first hypothesis; runtime
  corruption or host issues remain unexcluded. Nothing here connects this user
  process SIGSEGV to the older PVE kernel soft-lockup incident.

Oracle describes compiler-thread crashes as a possible compiler bug and cautions
that a workaround is not a fix. That supports the investigation direction but
not attribution to a particular defect. [Oracle JDK 21 crash guide](https://docs.oracle.com/en/java/javase/21/troubleshoot/troubleshoot-system-crashes.html)

### Concrete next proposal (not yet performed)

1. Add an explicit Java executable option to the isolated trial controller, and
   record its path/version, server JAR hash, flags and actual process return code.
   Its current final `server stopped` message alone is not proof of normal exit
   when the process has already crashed. Preserve all original failure evidence.
2. Use a project-local, checksum-verified **Temurin 21.0.12.1+1** installation for
   one fresh generated-roof acceptance trial, with normal tiered compilation.
   Leave the system Java selection and packages alone. Retain the same Minecraft
   JAR, native pin, construction plan requirements, heap/CPU limits and operation
   admission gates. This is a recovery candidate, not a claimed known fix; both
   update level and distribution would differ, so success cannot isolate which
   difference mattered or prove the old crash resolved universally.
3. Require the entire generated plan to complete, remove all owned temporary
   blocks, retreat and pass independent final observation. Also check client
   liveness through planning and actual server exit status. An idle server launch
   or a completed preflight alone does not clear stage 2.
4. If a compiler crash recurs, stop acceptance work again. A separate diagnostic
   comparison with `-XX:TieredStopAtLevel=1` can bypass C2, but may reduce throughput
   and is not proof that the original compiler defect is fixed. Do not silently
   adopt C1-only results as normal-C2 performance evidence or loosen operation
   deadlines. All-interpreter `-Xint` and compile replay are later diagnostic
   options, not the first recovery attempt.

Temurin's release announcement confirms 21.0.12.1+1 availability and describes it
as a security-only update; it does not establish a fix for this compiler crash.
The local, unrefreshed apt cache also advertises Ubuntu
`21.0.12.1+1-1~26.04.4`; no package refresh or installation occurred.
[Adoptium release announcement](https://adoptium.net/news/2026/09/eclipse-temurin-8u504-110321-170201-210121-25041-26021-available)

`TieredStopAtLevel` bounds compilation level and can be used to confine compilation
to C1 for diagnosis; disabling tiered compilation altogether is not the same
thing as disabling C2. [Microsoft OpenJDK compiler explanation](https://devblogs.microsoft.com/java/how-tiered-compilation-works-in-openjdk/)

The proposal above was subsequently authorized and its Temurin recovery trial
passed, as recorded at the top of this document. No C1-only or replay trial was
needed. The original JVM root cause remains undetermined; later roadmap stages
remain incomplete.
