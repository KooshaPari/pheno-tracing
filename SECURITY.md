# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

This project adheres to Semantic Versioning. Security patches are issued for
the current minor release line only.

## Reporting a Vulnerability

**Please do not file a public GitHub issue for security vulnerabilities.**

Instead, use one of the following channels (in order of preference):

1. **GitHub Private Security Advisory** (preferred):
   <https://github.com/KooshaPari/pheno-tracing/security/advisories/new>
2. **Email**: **security@pari.io** (PGP key on request; encrypted by default)
3. **Direct message** to the maintainer via GitHub: [@KooshaPari](https://github.com/KooshaPari)

Include in your report:

- A clear description of the vulnerability and its impact
- Reproduction steps (a minimal failing test is ideal)
- Affected version(s) and commit SHA(s)
- Any known mitigations or workarounds
- Your contact details for follow-up

## Response Timeline

| Stage              | SLA                   |
|--------------------|-----------------------|
| Acknowledge report | within 72 hours       |
| Triage & severity  | within 7 days         |
| Patch for CRITICAL | within 14 days        |
| Patch for HIGH     | within 30 days        |
| Patch for MEDIUM   | within 90 days        |
| Patch for LOW      | next regular release  |

We follow [CVSS 4.0](https://www.first.org/cvss/) for severity scoring.

## Disclosure Process

1. Reporter submits via one of the channels above.
2. Maintainer acknowledges, assigns a tracking ID (`SEC-pheno-tracing-NNN`),
   and opens a private advisory.
3. Triage, fix, and review happen in the private advisory.
4. A coordinated public disclosure date is agreed with the reporter.
5. At disclosure: a CVE is requested (via GHSA), the patch is released, and
   `CHANGELOG.md` is updated with a `Security:` block crediting the reporter.

## Scope

In scope for security reports:

- Memory unsafety in any `pub` API of `pheno_tracing::*`
- Denial-of-service vectors (e.g. unbounded span/attribute growth)
- Subtle correctness bugs in `TracePort` trait semantics
- `unsafe` code (currently zero — but flag any introduction)
- Supply-chain issues: malicious or compromised transitive dependencies
  detected by `cargo audit` / `cargo deny`
- OTLP export that could leak PII or secrets in span attributes

Out of scope:

- Bugs requiring a malicious config file under attacker control of the host
- Issues only reproducible against unreleased commit SHAs older than 30 days
- Theoretical issues without a working PoC

## Recognition

We follow a [Hall of Fame][hof] approach — reporters are credited in the
advisory and the `CHANGELOG.md` Security block (with their permission).

[hof]: https://github.com/KooshaPari/pheno-tracing/security/advisories

## Cryptography

This crate does not implement cryptography. Tracing spans and attributes are
plain text by design; **never** put secrets (API keys, tokens, passwords)
into span fields or attribute values.

## Acknowledgements

This policy is adapted from the
[GitHub Security Lab](https://securitylab.github.com/) and
[Coordinated Disclosure](https://vuls.cert.org/confluence/display/CVD/) best
practices.
