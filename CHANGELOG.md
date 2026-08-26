# Changelog

All notable changes to `pdf-core` are documented here.

## Unreleased

The next release will be cut after the full CI and signed-tag checklist passes.

## 0.1.1-alpha

- Consolidate the repository under `pdf-core` while keeping the `pdf-ast`
  crate name for compatibility.
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
- Added an optional veraPDF acceptance-comparison harness for the checked-in
  corpus.
- Removed fabricated JBIG2, signature, parallel-analysis, and cipher results;
  unsupported paths now return explicit errors or diagnostics.
- Replaced non-empty-password authentication with derived PDF R2-R4 password
  verification and propagated OS randomness failures from AES encryption.
- Applied the shared decode budget to legacy xref stream parsing and added a
  page-tree fuzz target to the CI campaign.
- Added cross-platform binding artifacts to the signed release workflow and
  documented that registry publication remains disabled.
- Reject malformed and overflowing DER lengths in the fallback certificate
  parser, with a truncation regression test.
- Removed the stale Python lockfile so workspace dependency resolution uses the
  audited root lockfile consistently.
- Fixed builds without default features by using sequential fallbacks when
  Rayon is disabled, and added a CI regression job for that configuration.
- Bounded object-stream allocations, xref scans, parser re-reads, and JPX,
  LZW, RunLength, ASCII, and JPEG decoder output.
- Made workspace Clippy warnings fatal in CI.
- Raised the MSRV to Rust 1.88.0 to retain the audited `time` dependency.
- Made differential corpus gates fail on missing CI tools or parser divergences.
