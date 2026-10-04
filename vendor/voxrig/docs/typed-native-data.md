# Typed native data and external text

Voxrig's production Rust library does not depend on `serde_json`. Registry,
collision, outline and received text-component interpretation use Rust types.
`serde_json` is a development dependency for independent pinned fixtures and
historical comparison tools; those records are not live operation capabilities.

## Registry and geometry

`scripts/generate_rust_tables.py` reads the pinned `data/` inputs offline and
writes `src/tables/java_1_16_1.rs` and `java_1_21_11.rs`. Each generated file records
input SHA-256 digests. Generation is explicit, never a build script or network
request. Runtime consumers borrow these compiled constants without embedding or
parsing registry JSON. Retain the input data and `THIRD_PARTY_NOTICES.md` for
provenance and independent regression checks.

```sh
python3 scripts/generate_rust_tables.py
python3 scripts/generate_rust_tables.py --check
```

Property domains distinguish booleans, integers and named enums. Native state
IDs and mixed-radix property order remain version-specific. Materials, tools and
recipe outputs use numeric IDs, rather than encoded string keys. Unsupported
outline states remain `None`, distinct from a supported empty shape. Collision
coordinates, shape order and state mapping preserve the pinned inputs exactly;
this migration adds no new physical or targeting coverage.

## Java 1.16.1 text: external wire exception

The server's chat/UI text is a JSON string in this version's native protocol.
`text_component::ProtocolText` preserves that external payload opaquely. It does
not parse a general JSON tree, render extensions, transport internal commands or
grant an operation capability. Its private storage distinguishes unavailable
text (`Default`, such as a horse window without a title) from received empty
text. `as_wire_json()` exposes the original payload explicitly at the caller's
presentation boundary. This exception does not introduce an internal JSON RPC.

The text API intentionally changes without legacy forwarding fields:

| Previous field | Current field/type |
| --- | --- |
| `ChatMessage.json` | `component: ProtocolText` |
| UI/window/advancement `title_json`, `description_json` | `title`, `description`: `ProtocolText` |
| `display_name_json`, `display_json`, `prefix_json`, `suffix_json` | corresponding names without `_json`, using `ProtocolText` (optional where previously optional) |
| title/subtitle/action bar and tab header/footer `_json` fields | corresponding names without `_json`, using `ProtocolText` |
| combat `message_json`, completion `tooltip_json` | `message: ProtocolText`, `tooltip: Option<ProtocolText>` |
| `Event::Disconnected.reason: String` | `DisconnectReason::Local` or `DisconnectReason::Server(ProtocolText)` |

The wire admission/framing behavior is unchanged. Local error diagnostics remain
ordinary error text, distinct from received server text. Examples inspect an
external chat payload through `as_wire_json()` rather than a legacy raw field.

## Java 1.21.11 text: native NBT

`SystemMessage.component` is `Option<text_component::TextNbt>`, not a JSON value.
The closed native variants retain integer widths, list element tags, byte/int/
long array kinds, ordered elements and compound fields. Duplicate field names
retain the last value, while every received field still consumes the existing
budget. A serialized diagnostic component now exposes `tag`/`value` native
variants; consumers must not index it as the former JSON projection.

Limits remain depth 32 and 16,384 values for component interpretation and 64 KiB
for packet projection. Native framing is validated first, including the overlay
flag and full payload consumption. Valid but unprojectable text retains its
receive sequence and overlay with `component: None`; malformed frames do not
change history. The 128-message queue and dropped-through sequence are unchanged.
`literal_text()` retains the established narrow rule: strings or a `text` string
with no translation and no nonempty/non-array `extra`. It is not a renderer.

`TextNbt` is serializable for output, not deserializable into live authority.
Moving-piston NBT keeps its existing purpose-specific parser and validation.
Scenes, watches, intents, receipts, connection retirement and their ownership
boundary are unchanged. No live server operation is required to regenerate or
verify these tables.

## Offline verification

The migration passed 212 library tests (8 live/server-dependent tests ignored),
2 ordinary doctests and 10 compile-fail doctests. All-target Clippy passed with
`-D warnings`. Generated-table `--check`, formatting, diff whitespace checks and
package contents passed. The normal dependency tree excludes `serde_json`.
Independent pinned-fixture tests compare every native state in both adapters,
every shape-coordinate bit/state mapping/unsupported outline, every recipe and
all consumed item, mining, material, entity and sound fields. Received text tests
cover native kinds, truncation, duplicate-field budget, depth/value/byte limits,
malformed frames and queue sequence. This is offline regression evidence, not a
new live-server validation.
