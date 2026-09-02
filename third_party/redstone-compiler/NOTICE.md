# Redstone Compiler attribution

`dustroute-translate::cells::external_xor_cell` is a coordinate-normalized
derivative of `test/xor-generated.nbt` from
<https://github.com/Redstone-Compiler/redstone-compiler> at commit
`cc997732b82d957a8b5cc80d14c07b375562dd9d`.

The two input levers in the source fixture are represented as external input
ports rather than reusable-cell blocks. The resulting layout was tested but
rejected for automatic use on Minecraft Java 1.21.11; retaining it supports
provenance and compatibility regression tests.

The upstream work is distributed under the MIT License reproduced in this
directory.
