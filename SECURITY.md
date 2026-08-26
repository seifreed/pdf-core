# Security Policy

## Scope

`pdf-core` parses untrusted PDF input and is experimental. Applications should
run it in a separate process with an application-level CPU, memory, and file
size policy.

## Reporting a Vulnerability

Do not disclose parser crashes, denial-of-service cases, sandbox escapes, or
malicious-PDF bypasses in a public issue. Use GitHub's private vulnerability
reporting for this repository:

https://github.com/seifreed/pdf-core/security/advisories/new

Include the smallest reproducing file or a minimized input, the affected
commit or release, platform/toolchain, expected behavior, and observed impact.
If the input is sensitive, describe how it can be shared privately instead of
including it in the report.

## Response Targets

- Acknowledge a report within 7 days.
- Triage reproducibility and impact within 14 days.
- Coordinate a fix, regression test, and disclosure timeline with the reporter.

Supported releases and published artifacts will be listed once signed GitHub
releases exist. Registry publication is gated by the `PUBLISH_REGISTRIES`
repository variable and registry credentials; until a release passes that
gate, the `main` branch and bindings are development artifacts. A release also
requires the `RELEASE_SIGNING_KEY` secret and matching
`RELEASE_SIGNING_FINGERPRINT` repository variable.
