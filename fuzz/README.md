# Fuzzing

Install `cargo-fuzz`, then run:

cargo fuzz run pdf_parse -- -runs=1000
```

Targets cover the lexer, indirect objects, document parser, object values,
content streams, filters, xref tables, object streams, page trees, CMap,
XMP, CMS/certificates, and AST serialization. Run a focused target with:

```bash
cargo fuzz run object_values -- -runs=1000
```

Crash inputs belong in `fuzz/artifacts/` and should become regression tests.
ClusterFuzzLite uses the checked-in `.clusterfuzzlite/` integration on pull
requests.
