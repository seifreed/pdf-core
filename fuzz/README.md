# Fuzzing

Install `cargo-fuzz`, then run:

```text
cargo fuzz run pdf_parse -- -runs=1000

Targets cover the document parser, object values, content streams, filters,
xref tables, and AST serialization. Run a focused target with:

```text
cargo fuzz run object_values -- -runs=1000
```
```

Crash inputs belong in `fuzz/artifacts/` and should become regression tests.
ClusterFuzzLite uses the checked-in `.clusterfuzzlite/` integration on pull
requests.
