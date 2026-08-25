# Fuzzing

Install `cargo-fuzz`, then run:

```text
cargo fuzz run pdf_parse -- -runs=1000
```

Crash inputs belong in `fuzz/artifacts/` and should become regression tests.
