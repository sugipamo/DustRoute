# Live piston interruption conformance

Historical evidence: this records the retired horizontal model comparison.
The old runner, comparator and dedicated actor have been removed. Its initial
fixture omits wire connection arms and cannot initialize the current electrical
model without new observation. The captured evidence below is unchanged; it is
not a current electrical conformance result. See
[electrical observations](piston-electrical-live-evidence.md) for current evidence.

The horizontal piston callback profile was compared with a fresh Minecraft Java
1.21.11 server on 2026-09-21. The checked circuit is the existing
`07-single-input-two-row` fixture. It contains two opposing sticky pistons and
uses the existing lever input. The live actor reconstructs all 55 fixture blocks
in an otherwise empty, force-loaded region, reads the placement back, applies
ON and OFF through normal player interaction packets, observes the result, and
cleans the region after every trial.

The comparison passed all six trials. The authoritative interval is the
difference between the two input ticks recorded by the server instrumentation;
client physics ticks are retained only as transport requests.

| Trial | Requested client delay | Applied server delay | Live/model result |
| --- | ---: | ---: | --- |
| Same tick | 0 | 0 | Match |
| Next tick | 1 | 1 | Match |
| Next tick, repeated | 1 | 1 | Match |
| Two client ticks | 2 | 1 | Match at the observed 1-tick interval |
| Completion boundary | 7 | 7 | Match |
| Settled | 12 | 12 | Match |

At a one-game-tick interruption, both piston bodies retract and the two moved
stones remain at the central output locations. Repeating the trial produced the
same applied interval and result. At the seven-tick completion boundary, the
sticky pistons retract and pull the stones back. The twelve-tick settled case
has the same final placement as the completion-boundary case. Same-tick OFF
cancels the extension. These results match
`dustroute.horizontal-piston-callbacks.java-1-21-11.v1` at the selected lever,
piston body, source, and output coordinates.

The bounded capture intentionally filters global records outside its pre-roll
and drain window, so `sequence_contiguous` is false. This means the artifacts do
not prove that no unrelated world event occurred. Each accepted trial does
contain exactly two server-applied input records, a closed capture, complete
client observations, valid retained piston/neighbor/state streams, and a model
run driven by the observed input interval. No re-ON trial was needed to resolve
the OFF boundary measured here; the existing source-derived re-ON behavior
remains a separate regression.

The first completion-boundary validation exposed a contract bug: stable sticky
pistons with a head were rejected because the validator admitted only normal
piston bodies. The validator now admits both stable piston variants and retains
the head requirement. The instrumentation was also corrected before the final
run so sequence allocation and emission share one lock, and network input is
recorded only when applied on the server thread. The final run uses those fixes.

## Retained evidence

The historical runner started a fresh Fabric server per trial, normalized and
validated the server artifact, and ran the then-pinned horizontal model with
the applied interval. Full runtime logs stay local under `.local/e2e-artifacts/`.
The tracked
[metadata](../crates/dustroute-translate/tests/fixtures/piston_interruption_1_21_11.meta.json)
retains the environment, source identity, artifact hashes, applied intervals,
and classifications from the accepted run.

This establishes final-state conformance for this fixture and these six applied
intervals. It does not establish complete Minecraft behavior, arbitrary
tick-internal ordering, vertical pistons, entities, other versions, or live-world
placement permission.
