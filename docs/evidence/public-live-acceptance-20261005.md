# Public live acceptance evidence — 2026-10-05 UTC

These are explicit trial captures, not production JSON storage or replay authority.
Final acceptance: building-b and regions-a, source `1590f5c`.
Building-a at `67e575c` passed its original lifecycle assertions but exposed missing
guard coordinates. It is retained as the pre-correction result, excluded from the
final coordinate acceptance. No failed process or unapplied input was relabeled.
All three probes and servers exited zero; all owned regions were restored to air.
See [scope and interpretation](../public-live-acceptance.md).

| Run | MCP calls | Expected refusals | Checkpoints | Independent cell comparisons | Normal MCP exits |
| --- | ---: | ---: | ---: | ---: | ---: |
| building-a | 56 | 10 | 18 | 28,224 | 2 |
| building-b | 56 | 10 | 18 | 28,224 | 2 |
| regions-a | 42 | 3 | 11 | 86,168 | 3 |

All 154 recorded MCP frames, including the pre-correction trial, agree with
their top-level `ok` field: failures set `is_error`; successful reads do not.
The final pair contains 98 calls, 13 expected refusals, 29 checkpoints and
114,392 independently compared cells. Refusals are declared test outcomes,
not failed trials. Saved manifests retain executable, source, JAR and Java
fingerprints, bounded JVM command, timestamped console checks and limits.

| Capture | SHA-256 of gzip | SHA-256 of uncompressed data |
| --- | --- | --- |
| [public-live-20261005-building-a.manifest.json.gz](public-live-20261005-building-a.manifest.json.gz) | `53b6a673f5967014f027971b89b61aac89569fdb97d49f898a9abeefe99ae93d` | `7989881b0aa63d78f47cfb9579aaca48aecb0e00b4d67b32a3011eb8827492f6` |
| [public-live-20261005-building-a.jsonl.gz](public-live-20261005-building-a.jsonl.gz) | `86308e0f939122d5713dd14580b586e26909272be167c6ea0702b1565bfe4e29` | `dfc1a5cd92054a6d3080767be666be184a2cc04715d5b0f07f62b4ab30ffe091` |
| [public-live-20261005-building-a.server.log.gz](public-live-20261005-building-a.server.log.gz) | `9fc4bf9bb01994f31d8463d66bba45c272d1bbb239ab8af0ce42f0f9ec072127` | `0c39bcf2500a0bda9bb3fac643dc1e9cb30298f6ee845db2c23658c6628b4e3f` |
| [public-live-20261005-building-a.probe.log.gz](public-live-20261005-building-a.probe.log.gz) | `52e0561f02a50540832985c321a70b5b26a174afae102fff358cd167392e29ac` | `b79e7734c20de2f4ec65c96ca3129e1b6eb55d13cda6d6fc5895ec3d58f37870` |
| [public-live-20261005-building-b.manifest.json.gz](public-live-20261005-building-b.manifest.json.gz) | `a94b75c488c1006770b4e995bb56ca6fd75351a235f09b56d2ff40438f2c25c5` | `6245155b454974f57e04e4a28379c49eb56d98a791d378d95375e95523d3a7a6` |
| [public-live-20261005-building-b.jsonl.gz](public-live-20261005-building-b.jsonl.gz) | `949cadc0974bf3b674b1cc5249fe8f8c9c65372a73c77d992d4d00c554326a61` | `ee8798ff92c45fc0f01ef742d899cce129f2f226c9196063f43c50f4249df6c3` |
| [public-live-20261005-building-b.server.log.gz](public-live-20261005-building-b.server.log.gz) | `9dd30a39a04284b29782e7add52552f26649929776ea3a62ba68293ddb0e3ee1` | `a5876956d5ae8eccd0dd2ae0d9609e9db9428666f864c405a46061d4fbde7e35` |
| [public-live-20261005-building-b.probe.log.gz](public-live-20261005-building-b.probe.log.gz) | `3ad5d513ad2e92b375cd9b584460bc23febe2a02dee2b6df54fef974c3f275fb` | `a0bda3ec9327e4a5073e6eb3d23beca6952a73cdcbbd855e47b14867a0014e96` |
| [public-live-20261005-regions-a.manifest.json.gz](public-live-20261005-regions-a.manifest.json.gz) | `3591f66f1042b46049a6f0232787515f873a7d4936276859f78e37d95792061b` | `19ec0fd928e879ad28439013a2a8b026455d61069032eab5342c7e08de3069d1` |
| [public-live-20261005-regions-a.jsonl.gz](public-live-20261005-regions-a.jsonl.gz) | `309bf8745e19e0fa66aa69811e2a4d95f4b7acdacc09bea99a8ca590cb930242` | `1cc92aab92a13f564c38290c69ca70422c12fe1f57abdcf01be59dbd977ad17c` |
| [public-live-20261005-regions-a.server.log.gz](public-live-20261005-regions-a.server.log.gz) | `e9c61876c77b2211b500ca50d3ec08536b91de9c91e3be812c292f40c48e456a` | `fbdfe081f10f7a02a10bca883ea9fe6193eb80676b569f2dc0f3ba669854bd81` |
| [public-live-20261005-regions-a.probe.log.gz](public-live-20261005-regions-a.probe.log.gz) | `a4f85d01284b8ae5df96136f3daccd35de9de53ac219f2201fad397fcda26801` | `26578a257a62a4b2e05ed4d87b501e68ae6356f0ca67cbc2a683a3975b6b4d1c` |
