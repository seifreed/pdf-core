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

The `0.1.x-alpha` API is experimental. SemVer-compatible releases may add
items, but may still revise experimental modules before beta. Stable modules
will not remove or change public items without a deprecation period and a
documented migration note.

## AST Serialization

The AST serialization schema is currently `1.1.0` and is experimental. The
deserializer migrates the historical `1.0` envelope when object identities are
complete; unknown, inconsistent, or incomplete versions are rejected. Serialized
object identities, node types, edge types, source offsets, source sizes, and
schema version must remain self-describing rather than inventing values.

## Security Scope

Compatibility does not imply security support for experimental codecs,
bindings, compliance checks, or recovery heuristics. Those surfaces may
change independently while they remain marked experimental in the README.
