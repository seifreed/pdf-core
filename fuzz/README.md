# Fuzzing

Install `cargo-fuzz` and the nightly Rust toolchain, then run:

```bash
cargo +nightly fuzz run pdf_parse -- -runs=1000 -timeout=5 -print_final_stats=1
```

Targets cover the lexer, indirect objects, document parser, object values,
content streams, filters, xref tables, object streams, page trees, CMap,
XMP, CMS/certificates, and AST serialization. Run a focused target with:

```bash
cargo +nightly fuzz run object_values -- -runs=1000 -timeout=5 -print_final_stats=1
```

Nightly is required because cargo-fuzz uses Rust's sanitizer instrumentation
flags.

Crash inputs belong in `fuzz/artifacts/` and should become regression tests.
ClusterFuzzLite uses the checked-in `.clusterfuzzlite/` integration on pull
requests.

The latest local 16-target smoke result is recorded in
[`LOCAL-SMOKE-RESULTS.md`](LOCAL-SMOKE-RESULTS.md). It does not replace the
GitHub Actions and ClusterFuzzLite gates.
