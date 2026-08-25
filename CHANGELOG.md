# Changelog

All notable changes to `pdf-core` are documented here.

## Unreleased

- Consolidate the repository under `pdf-core` while keeping the published
  `pdf-ast` crate name for compatibility.
- Mark parser, schema, compliance, bindings, and auxiliary tools as
  experimental where coverage is incomplete.
- Add a Cargo workspace for the root crate, auxiliary crates, bindings, and
  plugin example.
- Hardened object, stream, xref, and object-stream parsing against truncation,
  invalid offsets, negative lengths, and fabricated recovery data.
- Added strict, tolerant, and forensic parser modes plus shared resource
  budgets with cancellation and deadlines.
- Added a checked-in regression corpus, cargo-fuzz target, corpus CI job, and
  scheduled/PR fuzz smoke campaign.
- Versioned AST serialization as 1.1.0, restored object identity on
  deserialization, and qualified compliance output as experimental preflight.
- Upgraded audited dependencies and verified cargo audit --deny warnings.
