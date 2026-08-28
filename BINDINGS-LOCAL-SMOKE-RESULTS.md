# Local Binding Smoke

This is local evidence, not a hosted CI or registry publication result.

- Date: 2026-08-26 UTC
- Host: macOS arm64
- Python: 3.14.7
- Node: 26.7.0

Checks passed:

- Python wheel built with maturin, installed into a clean virtual environment,
  and passed `python/test_bindings.py`.
- Node native binding built with `npm run build-release`, passed `npm test`,
  and passed the packaged install smoke test.
- C ABI library built and `tests/ffi_header_smoke.c` compiled and ran against
  the generated dynamic library.

The GitHub Actions binding matrix remains the authoritative cross-platform
check; release publication remains disabled until its credentials and signed
tag requirements are configured.
