# Compatibility Policy

## Rust

The minimum supported Rust version (MSRV) is **1.88.0**. The root
`Cargo.toml` and CI MSRV job are the authoritative declarations. A change
that requires a newer compiler must update both in the same change.

## Crate Identity

The repository and project are named `pdf-core`. The published crate is
currently named `pdf-ast` for compatibility with existing consumers. Renaming
the crate is a breaking change and requires a major-version migration plan.

## Rust API

The `0.2.x-alpha` API is experimental. The `0.2.0-alpha.1` line deliberately
contains the lossless AST and parser-state fields added after the published
`0.1.0` baseline. The API audit compares this line against `0.1.0`; future
breaking changes require another minor-line bump while the crate remains
below `1.0.0`. Stable modules will not remove or change public items without
a deprecation period and a documented migration note.

## AST Serialization

The AST serialization schema is currently `1.1.0` and is experimental. The
deserializer migrates the historical `1.0` envelope when object identities are
complete; unknown, inconsistent, or incomplete versions are rejected. Serialized
object identities, node types, edge types, source offsets, source sizes,
incremental revisions, and schema version must remain self-describing rather
than inventing values. `SerializableDocument::deserialize_ast` restores the
validated graph; it does not claim to reconstruct parser-only runtime state.

## C ABI

The C header exposes ABI version `1.0`, returned by `pdf_ast_abi_version()` as
`(major << 16) | minor`. Opaque document and node handles are owned by the
caller after successful creation; strings, result messages, and child arrays
must be released with the matching `pdf_ast_free_*` function. The ABI smoke
test compiles with `-Wall -Wextra -Werror`; incompatible C changes require an
ABI major bump.

## Security Scope

Compatibility does not imply security support for experimental codecs,
bindings, compliance checks, or recovery heuristics. Those surfaces may
change independently while they remain marked experimental in the README.
