# Security Policy

`pdf-core` parses untrusted PDF input and is still experimental. Run it in a
sandbox with resource limits when processing files from untrusted sources.

## Reporting a vulnerability

Do not open a public issue for a security vulnerability. Contact the
maintainer privately through the security contact configured for the
repository, including a minimal reproducer, affected revision, and impact.

Reports are acknowledged within seven days. Fixes and disclosure timing are
coordinated with the reporter.

## Scope

Reports covering panics, uncontrolled resource consumption, memory safety,
and security-analysis bypasses are in scope. Features marked experimental or
partial may produce incomplete analysis and should not be treated as a
security boundary.
