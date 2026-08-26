# MSRV and Compatibility

The minimum supported Rust version is **1.85.0**, matching `rust-version` in
the root `Cargo.toml`. The CI `msrv` job checks the complete workspace against
that toolchain.

The project is pre-1.0 and experimental. Public APIs, feature flags, AST
serialization, and diagnostics may change between minor releases. The AST
serialization schema is versioned independently and incompatible changes must
increment its schema version.

Dependency updates must preserve the declared MSRV or update this document,
`Cargo.toml`, and CI in the same change. Consumers processing untrusted PDFs
must still provide process isolation and application-level resource limits.
