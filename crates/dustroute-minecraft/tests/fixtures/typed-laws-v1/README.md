# Frozen builtin Law programs

These files preserve the complete programs used before the Rust-definition migration.
They are test inputs, never embedded production assets. Current definitions live in
`src/law/builtins`; the migration test compares every field and compiles each program.
Changing a current law requires an explicit revision and conformance decision, rather
than silently updating these reference programs.
