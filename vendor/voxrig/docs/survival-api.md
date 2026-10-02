# Checked survival API and ownership

`Client::survival_capabilities()` and
`checked_survival::SurvivalCapabilities::for_version(version)` describe static
adapter support. `Client::survival()` returns a session-bound checked handle, or
`Unsupported` before I/O for Java 1.16.1. Its legacy `Bot` and `survival` module
remain unchanged. Java 1.21.11 currently implements `ObservedDryCubeV1`.

Support is not readiness or permission. Each call still validates native state,
identity, generation, geometry, inventory and unresolved intents. The façade
forwards to the original implementation; it adds no physics, fallback, automatic
retry, relaxed admission or second operation history. Data types currently share
the native 1.21.11 representation and retain their version/session provenance;
this is not a promise that another version already implements these semantics.

| Voxrig | Consumer |
| --- | --- |
| Version selection and capability contract | Select supported deployment/version |
| Received state, collision, aim, bounded motion prediction | Choose route, building geometry and action sequence |
| Before-I/O intent and single-operation observations | Site/edit permissions and material reservations |
| Exact session retirement and fresh connection validation | Decide whether recovery is appropriate; replan afterward |
| Non-restorable in-process handles and diagnostic history | Durable jobs, restart diagnosis and user-facing errors |

The checked handle offers received player/history/standing observations,
hypothetical scenes, bounded walking/jumping, main/hotbar plain-stack swaps,
passive full-cube placement, empty-hand dirt/stone mining and motion reassessment.
It exposes no commands, creative inventory or raw packet sending. The explicit
version-specific API remains for other operations. See [movement](survival-movement-foundation.md),
[hypothetical scenes](survival-hypothetical-scenes.md), and
[mining retirement](survival-mining-retirement.md) for admission/evidence limits.

## Explicit mining lifecycle

1. `operations.prepare_mining_retirement(&intent, &observer).await` returns a
   `MiningRetirement` handle after registering the exact independent watch.
   No close or reconnect occurs in preparation.
2. Retain that handle before calling `close_source().await`. A cancelled close
   or wait does not authorize another mining action. `source_history()` remains
   accessible even when observer inspection fails.
3. `observe()` / `wait(duration)` return native pending, retired or inspection
   evidence, including the existing `recovery_started` marker. Waits are read-only
   and resumable on the same handle; dropping the handle performs no recovery.
4. Only after retirement, explicitly call `reconnect(config, expected_target)`.
   Native validation records the attempt before network I/O. Cloned handles share
   that guard; cancellation/failure cannot authorize a second login.
5. The result contains `client`, checked `operations` and `evidence` on the same
   fresh session. The consumer must inspect/replan its own work. No Blueprint,
   durable job, edit permission or material reservation crosses this boundary.

These are direct, unmodified vanilla 1.21.11 retirement semantics. The façade does
not expand support to proxies/plugins or infer retirement from missing entities,
wall time, target air, ABORT or local closure alone. Existing low-level methods
coexist for callers that explicitly manage the same lifecycle themselves.
