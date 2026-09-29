# Third-party notices

Voxrig includes generated registry data and test fixtures derived from the
following open-source projects. The Voxrig MIT license does not replace their
notices. Exact npm versions and registry integrity digests are recorded in
`reference/package-lock.json`.

## Locally recorded Java 1.21.11 diagnostics

`data/java_1_21_11/outline_*` contains factual shape coordinates, native state
coverage and numeric raycast/rotation observations from a locally obtained Java
1.21.11 game. Original Java/Python tooling invokes native APIs; it is not copied
game source. Yarn 1.21.11+build.6 names identify the inspected methods. These data
do not relicense Minecraft or Yarn; no game JAR, mapping or decompiled source is
included. Exact source hashes, transformation and regeneration instructions are
in `outline_source.json` and `docs/player-targeting.md`.

`docs/evidence/client-motion-*-20260929.json.gz` and their replay fixtures were
recorded by this repository's `packet_trace_probe` against an isolated official
Minecraft Java 1.21.11 server. The accompanying manifest identifies originals,
transformations, checksums and failed trials. Native state names/properties use
the pinned minecraft-data registry described below. These are local diagnostic
observations and original test tooling, not imported DustRoute source or fixtures.
The `client-players-*` records likewise come from two local clients on that
isolated server and include the failed/corrected native attribute-ID trial.
No Minecraft JAR or decompiled source is redistributed in this repository.
The inspected version and mapping identification are recorded in
`docs/client-piston-reconstruction.md`.

## User-supplied reference door diagnostics

`docs/evidence/client-reference-door-*-20260929*` records local observations of
Bobiloosky's One-Wide 3x3 Piston Door:
<https://www.planetminecraft.com/project/one-wide-3x3-piston-door-works-on-java-edition/>.
The user supplied world ZIP (SHA-256 recorded in the evidence manifest) was
previously inspected by DustRoute. Initial block coordinates/properties for
these local tests were read from DustRoute's `reference-3x3-observed-a-v1.json`
at commit `b1762b9`, translated by (-41900, 0, -900), and initialized on an
isolated server. No DustRoute implementation source or Minecraft binary is
included. These files are diagnostic records; Voxrig's MIT license does not
claim ownership of the circuit design or relicense the supplied world. Review
fixture redistribution attribution when preparing an upstream submission.

## minecraft-data 3.114.0

Upstream: <https://github.com/PrismarineJS/minecraft-data>

The Java 1.21.11 version adapter additionally uses `data/java_1_21_11/blocks.json`
(name, state ranges and complete property definitions), generated packet-ID
constants (including player registry IDs) in `src/versions/java_1_21_11/ids.rs`, and
`data/java_1_21_11/items.json` (native item ID, name and default stack size)
from the same exact package.
`data/java_1_21_11/collision_shapes.json` projects that package's block collision
shapes into native state IDs and AABBs. These are static collision shapes, not
graphical selection outlines or entity-dependent collision decisions.
`data/java_1_21_11/source.json` records the release, protocol, transformation and
generated block-data digest. These data do not depend on a running JavaScript client.

The files `data/blocks.json`, `data/items.json`, `data/materials.json`,
`data/entities.json`, `data/recipes.json`, `data/sounds.json`,
`data/block_state_ranges.json`, and `data/block_collision_shapes.json` contain
or are derived from Minecraft Java 1.16.1 registry data distributed by the
`minecraft-data-3.114.0.tgz` npm package. That exact package contains no
standalone license file. Its package metadata names
`Romain Beaumont <romain.rom1@gmail.com>` as author and declares the MIT
license; the upstream README also states MIT and cautions that individual
source data may require different terms.

MIT License

Copyright (c) Romain Beaumont and minecraft-data contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## prismarine-block 1.23.0

Source: <https://github.com/PrismarineJS/prismarine-block/tree/1.23.0>

`prismarine-block` was used by `reference/generate_fixtures.js` to construct
blocks supplied to the reference physics implementation. No
`prismarine-block` source code is embedded in Voxrig. The exact npm package
contains no standalone license file; its package metadata names
`Romain Beaumont <romain.rom1@gmail.com>` as author and declares MIT.

MIT License

Copyright (c) Romain Beaumont and prismarine-block contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## prismarine-physics 1.11.1

Source: <https://github.com/PrismarineJS/prismarine-physics/tree/1.11.1>

`data/prismarine_physics_fixtures.json` was generated by
`reference/generate_fixtures.js` using `prismarine-physics` 1.11.1,
`prismarine-block` 1.23.0, and `minecraft-data` 3.114.0. The notice below is
copied from the upstream `1.11.1` tag's `LICENSE` file.

MIT License

Copyright (c) 2020 PrismarineJS

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
