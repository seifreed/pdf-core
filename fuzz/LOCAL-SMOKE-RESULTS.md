# Local Fuzz Smoke

This is local evidence, not a GitHub Actions or ClusterFuzzLite result.

- Date: 2026-08-26 UTC
- Host: macOS arm64
- `cargo-fuzz`: 0.13.1
- Rust: `1.99.0-nightly (c98d0cb27 2026-08-12)`
- Campaign: 16 targets, 1,000 libFuzzer runs per target
- Total executions: 16,000
- Result: all targets passed; 0 crashes and 0 findings

Command:

```sh
for target in certificates cmap content_stream filters indirect_objects lexer object_streams object_values page_tree pdf_parse pkcs7 serialization streams xmp xref xref_streams; do
  cargo +nightly fuzz run "$target" -- -runs=1000
done
```

The campaign also exercises the checked-in seed corpus. Any future crash
should be retained under `fuzz/artifacts/` and promoted to a regression test.

Follow-up after JBIG2 decoder integration:

- Target: `filters`, 1,000 runs, `-timeout=5`, no crashes or findings
- Peak RSS: 295 MB
- Result: passed with the bounded JBIG2 preflight active
