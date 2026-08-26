# Release Policy

`pdf-core` is currently pre-1.0 and no stable compatibility guarantee is
provided. Releases are made from `main` only after the required CI checks pass
and the changelog describes user-visible changes and known limitations.

Release checklist:

- confirm `cargo fmt --all -- --check` and `cargo test --workspace` results;
- confirm the checked-in corpus and differential checks ran;
- confirm `cargo audit --deny warnings` is clean;
- review `COMPLIANCE.md`, `SECURITY.md`, and `MSRV.md` for claim changes;
- create an annotated, signed tag only after the commit is on `main`;
- publish checksums and provenance for every released artifact.

The repository does not currently publish crates.io, PyPI, or npm artifacts.
Bindings and release automation remain experimental until installation smoke
tests and reproducible multi-platform builds are available.
