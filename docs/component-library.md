# Component library

`dustroute-library` stores reusable circuit specifications separately from
Minecraft block semantics and translation. Its JSON-compatible schema records:

- a stable component ID and logical kind;
- named input/output ports and an exhaustive truth table;
- optional physical size, resource, and delay metrics;
- edition/version and update-order compatibility;
- author, source URL, license, and retrieval date;
- evidence supporting the claimed behavior.

Evidence advances through `published_claim`, `logical_exhaustive`, `simulated`,
and `minecraft_e2e`. A failed live compatibility probe is retained separately
as `minecraft_e2e_rejected`; it never promotes a component. Import validates every logical truth table. A physical
component is eligible for automatic replacement only when it has physical and
compatibility metadata, a stated license, and Minecraft E2E evidence. This
prevents an online diagram or quoted metric from silently becoming an
authoritative implementation.

The built-in catalog contains logic-only NOT, AND, OR, XOR, and half-adder
specifications. It also contains a conservative compiled XOR physical baseline
for Minecraft Java 1.21.11. That layout is generated from verified primitive
cells, exhaustively simulated with a 64-redstone-tick settling budget, and
verified for all four stable inputs by Mineflayer E2E scenario 26. It is large
(`51 x 5 x 13`, 341 occupied blocks) but is eligible for automatic replacement
and serves as the correctness reference for later compact XOR candidates.
Its observed output settles in 5--9 redstone ticks. A simultaneous `10 -> 01`
input exchange produces a temporary low output, so consumers that require a
hazard-free transition must add an explicit timing contract rather than relying
only on the stable XOR truth table.

The first spacing/lane optimization sweep found a smaller verified candidate at
`39 x 5 x 11` and 275 occupied blocks. It keeps the same logical expansion but
uses compiler spacing 9 and lane gap 6. Scenario 27 verifies it on Java 1.21.11;
it settles in 5--7 redstone ticks and shortens the `10 -> 01` intermediate low
pulse to three ticks. The cell library selects this compact candidate first,
while retaining the larger layout as the comparison oracle.

External catalogs can be decoded with `Catalog::from_json`. DustRoute does not
download sources from inside this crate; acquisition, license review, and
content storage remain explicit caller responsibilities.

The first external probe is Redstone-Compiler's MIT-licensed generated XOR at
commit `cc997732b82d957a8b5cc80d14c07b375562dd9d`. Its logical claim is a valid
XOR, but its published output remains low for every input combination in
Minecraft Java 1.21.11. After differential physics fixes, DustRoute reproduces
the same low output and all selected physical observations match Minecraft. It
remains catalogued as logically verified provenance with physical metrics,
explicitly incompatible with 1.21.11, and unavailable for automatic
replacement.
