# Explicit prediction-based dry-cube continuation

The checked Java 1.21.11 adapter offers `PredictedDryCubeV1` alongside
`ObservedDryCubeV1`. `prediction_based_contract` advertises the new contract;
the existing capability and independent-observer entry points keep their meaning.
The caller explicitly uses `start_predicted_survival_path` or
`start_previewed_predicted_survival_motion`. No observer is accepted or required
by these operations. The native model, bounded connection-owned dispatch task,
loading and send-interruption guards are shared with observed movement.

`SurvivalMotionStatus::Predicted` means all controls were dispatched and the dry
model ended with released input and floor contact. It does not mean the server
accepted every frame, that actual motion stopped, or that server position was
measured. `StandingPositionBasis::Predicted` retains the run, model frame and the
own-position receive ordinal preceding it. It never replaces the retained own
receipt or supplies an `ObservedPlayer`. The 1/16-block horizontal
`planning_reserve` is a construction policy around the model, **not a measured
physical error bound**. Floor, body, aim, reach and visibility are evaluated in
that model-space reserve against currently received admitted geometry.

Fresh checked standing admission requires the same generation, dimension,
pre-run own-pose receipt, player posture/default motion attributes, velocity and
effect context, complete unsuperseded final dispatch, model endpoint and current
dry floor support. Correction, impulse, changed context, unsupported geometry,
missing support, send interruption or incomplete reconstruction refuses
continuation. A retained candidate status cannot bypass that fresh check. Reading
the motion record latches changed dispatch/context as `RequiresInspection`.
There is no prediction-only observation recheck that clears a failed run and no
automatic replay. Fresh reconnect and caller replanning remain explicit.

Hypothetical plans choose the same policy with
`scenario_with_motion_contract(SurvivalMotionContract::Predicted)`.
Their future endpoints carry `HypotheticalAimRequirement::PredictedEndpoint`
and `HypotheticalMovementPreview::endpoint_contract`. A predicted endpoint
cannot satisfy `IndependentlyObservedEndpoint`; `validate_standing` compares a
freshly checked actual basis with the declared prospective requirement, granting
no action authority. Model gravity phase is retained across successive runs;
received reconnect starts remain a separate initialization and obligation.

Voxrig owns these physical/protocol conditions and operation history. The caller
still owns routes, build designs, supplied resources, temporary-block ownership,
durable plans, diagnosis and any reconnect/replan policy. Neither serialized
standing nor a serialized hypothetical requirement restores native permission.
The dry full-cube, bounded-input, no sprint/sneak/fluid/entity/tool/gathering scope
is unchanged. Placement still requires native target/material/sequence receipts;
mining still requires explicit retirement and fresh recovery before further work.

Four loopback regressions exercise absent position echoes, hypothetical/live
contract agreement, preserved model gravity phase, correction/impulse/generation
refusal, current support/body checks and interrupted dispatch. The whole native
library passed 202 tests with seven opt-in tests ignored before adding the new
opt-in live comparison driver; documentation checks and all-target Clippy passed.
Live source `c491f6a34fd94bd43eae06e2aa6ac8112d05e5f1` passed walk,
jump/landing and wall collision/retreat with three ordinary placements. The native
run had no observer watches or observations. The comparison observer received
the jump rise, matched each endpoint and independently confirmed all three
placed cubes. Test and isolated non-OP server exited zero; total controller time
was 38.52 seconds. This is bounded native acceptance, not integrated caller build,
mining-after-walking or separate-process continuation acceptance. Hashed traces,
controller and runtime metadata are retained in
[the live evidence manifest](evidence/survival-predicted-motion-live-20261003.json).
