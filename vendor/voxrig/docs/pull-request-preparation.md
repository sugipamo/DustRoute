# Pull request preparation

Working branch: `codex/native-client-usability`, based on upstream `6434b2c`.
Local `develop` retains the prior implementation through `b98785e`.
No branch push, upstream issue, or pull request has been submitted. The user
authorized substantial local Voxrig changes and requested preparation for a later PR.

Suggested title: **Add explicit Java version adapters and bounded 1.21.11 client piston updates**

Suggested description:

> Preserve the existing Java 1.16.1 Bot API behind an explicit version module and
> add a Java 1.21.11 adapter for native block observation. Piston block actions can
> change neighboring shapes without a corresponding state packet. The new client
> view reconstructs the supported piston effects while retaining the original
> received states, causal sequence, independent moving carriers and explicit
> incomplete-state errors.
>
> Offline isolated-server trials cover ordinary extension/retraction, sticky
> pulling and short-pulse drop retraction. Final 770-cell regions were independently
> checked against the server; packet/frame replay covers intermediate client
> states. Existing unit tests, examples, doctest, formatting and Clippy checks are
> retained. Follow-up commits add ordered slime/honey branches, reference-door
> callbacks/reload, bounded creative construction controls, inventory knowledge
> and remote-player packet observation. Static collision targeting and bounded
> block-update recordings preserve their geometry, clock and packet provenance.
> Audited static outline targeting additionally selects thin circuit components,
> with native raycast/rotation oracle tests and explicit unsupported geometry.
> This does not claim a graphical camera-frame receipt, entity physics,
> survival pathfinding, online authentication or command-free server confirmation.

Review order:

1. Version isolation and preservation of the existing 1.16.1 API (`4c588d2`).
2. Native 1.21.11 receiving, pinned registries and original discrepancy (`bb7cd23`).
3. Client-only reconstruction, its provenance contract and moving-state lifecycle (`195917e`).
4. Live captures, deterministic replay and the supported/unsupported boundary.

The client behavior and tests have no DustRoute runtime dependency. Before an
upstream submission, follow `CONTRIBUTING.md`'s maintainer discussion process,
review the public API/compatibility boundary, and decide whether to split the
version adapter and reconstruction commits into separate PRs. Do not mark the
known 1.16.1 two-bot movement trial as resolved without a baseline comparison.
Local commits currently use the environment's automatically selected author;
confirm the intended author identity before an upstream submission.

The acceptance evidence and remaining limits are in
[client-piston-reconstruction.md](client-piston-reconstruction.md) and
[version-adapter-validation.md](version-adapter-validation.md).
