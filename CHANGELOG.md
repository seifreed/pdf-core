# Changelog

All notable changes to `pdf-core` are documented here.

## Unreleased

The next release will be cut after the full CI and signed-tag checklist passes.

- Expanded the pinned corpus to 2,806 upstream PDF/A, PDF/UA, and Isartor
  fixtures, with SHA-256 manifest coverage and exact veraPDF rule mappings.
- Added complete veraPDF mappings for all 205 Isartor fixtures and a local
  16-target fuzz smoke report covering 16,000 executions.
- Recorded corpus-gate memory and latency metrics for all 2,809 fixtures,
  including a measured peak RSS of 197168 KiB after codec integration.
- Added the ISO 32000 clause inventory, serialized veraPDF mapping test, C ABI
  header/link smoke test, and 60 parser-fuzz seeds from a 1,000-run campaign.
- Classified tagged-PDF `/StructTreeRoot` and `/StructElem` nodes during
  reference resolution and added serialized PDF/UA structure and language
  rule coverage.
- Preserved inherited page resources, exercised hybrid xref tables/streams,
  and improved variable-width ToUnicode decoding including UTF-16 surrogate
  pairs.
- Versioned the C ABI as 1.0 with documented ownership and warning-free header
  smoke coverage.
- Added bounded pure-Rust JBIG2 and JPEG 2000 pixel decoding with regression
  fixtures; PDF `/JBIG2Globals` plumbing and full codec conformance remain
  pending.
- Preserved direct `/JBIG2Globals` bytes through stream filter parameters;
  parsed indirect document references are now resolved before stream decoding.
- Added JBIG2 segment, bitmap-dimension, and organization preflight so
  unbounded inputs are rejected before the decoder allocates or iterates.
- Added bounded validation for standalone random-access segment organization.
- Made the Node release artifact portable through platform-specific optional
  native packages and a checked-in platform loader.

## 0.2.0-alpha.1

- Bumped the workspace API line after adding lossless parser state and
  diagnostics to public AST and serialization structures.
- Made Python and Node binding version constants follow the Rust package
  version and made the semver audit compare explicitly with `0.1.0`.

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
- Made TSA chain and revocation checks explicitly opt-in in the library and CLI.
- Locked JavaScript binding dependencies and switched binding workflows to `npm ci`.
- Added corpus latency percentiles and rejected malformed xref, image, and function dimensions safely.
- Deserializers now reject unsupported AST schema versions and inconsistent graph metadata instead of silently repairing input.
- Added an explicit AST `1.0` to `1.1.0` migration and fuzz coverage for JSON/CBOR deserialization.
- Compliance adapter reports now preserve rule locations, source offsets, expected actions, and registry ISO references.
- Hardened color-space and page-tree array parsing against malformed input without unchecked indexing.
- Hardened predictor dimension arithmetic against zero values and integer overflow.
- Rejected overflowing ASCII85 tuples and fuzzed every stream-filter variant.
- Preserved AST node source offsets and sizes through graph serialization.
- Added JSON/CBOR document envelope round-trips for AST graphs and revisions.
- Added dedicated fuzz targets for indirect streams and xref streams.
- Removed the obsolete commented streaming placeholder from the CLI.
- Added the pinned `pdf-core-corpus` fixture repository and corrected veraPDF
  comparison to separate parser acceptance from PDF/A conformance.
- External corpus tests now verify downloaded SHA-256 manifests before parsing.
- veraPDF comparison now runs as one batch over the pinned corpus and separates
  tolerant parsing from strict-mode rejection metrics.
- Fixed the corpus benchmark to ignore non-PDF metadata files.
- Preserved the pre-0.1.0 public constructor signatures for xref, color-space,
  document, and file parsers while retaining the bounded `*_with_limits` APIs.
- Added a semver audit against the published `0.1.0` baseline; public struct
  field additions remain an intentional API-freeze blocker for the next major
  compatibility milestone.
- Added `SECURITY.md` with private vulnerability-reporting guidance and
  deployment isolation requirements for untrusted PDFs.
- Verified the Python wheel install smoke test and Node native binding smoke
  test locally; registry publication and signed releases remain disabled.
