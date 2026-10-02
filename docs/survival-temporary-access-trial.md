# Declared temporary access trial

This is the next isolated non-OP 1.21.11 access milestone in the survival
Blueprint roadmap. It does not replace the adopted roofed-structure acceptance
or implement a general construction scheduler/durable job.

Before any player mutation, the test captures the prepared static scene and
checks this entire finite geometric sequence with the shared native predictor:

1. From feet `[0.5,-60,0.5]`, place dirt at `[3,-60,0]`, `[2,-60,0]`,
   `[1,-60,0]` by clicking the existing stone floor's top faces. This declared
   far-to-near order preserves the selected face visibility.
2. Eight eastward input ticks with an initial jump, then twenty released ticks,
   must reach a resting platform position at y=-59 with terminal clearance.
3. Eight westward walking ticks and twenty released ticks must reach a safe
   ground standing position at y=-60.
4. From that retreat, remove the three dirt cubes near-to-far. Every hypothetical
   removal must have the native first-hit face and retain standing support.

The test supplies exactly three dirt in main inventory. Predicted item drops are
never credited toward resources. Ordinary placement requires received material
decrements plus independent target observations. Each real motion recomputes its
native live preview and compares every frame to the earlier hypothetical result
before dispatch; an independent observer must then establish the actual endpoint.

For each cleanup cube, the caller re-captures and re-plans removal from current
standing and inventory, chooses a received empty hotbar slot, invokes ordinary
mining and independently observes air. It then closes the miner, waits for the
observer's exact profile-removal receipt and invokes the existing explicit
recovery API. `MiningRecovery::client()` exposes the same validated fresh
connection for observation, tracing and explicit closure; it does not perform
another login or release operation guards. The next removal uses new evidence.

Completion checks every cell in `[-1,-61,-1]..[4,-59,1]`: stone throughout the
bottom plane, exact air above, including all temporary targets. Received traces,
retirement/recovery records and inventory outcomes are retained. The fixture
console prepares floor/air/starting poses/materials before the exercise; the
builder does not send commands, switch modes, teleport or create materials.

The opt-in test is
`survival_navigation::native_access_trial::native_temporary_access_place_climb_retreat_cleanup`.
Its presence/compilation alone is not evidence of success. Evidence must pin the
tested source and preserve failures; no blind second placement/mining attempt is
allowed after an ambiguous result.
