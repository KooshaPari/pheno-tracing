# Threat Model — pheno-tracing

_Audit fix for L20 (v37 scorecard). Covers trust boundaries, data sensitivity,
unsafe-code posture, and backend-export assumptions._

## Scope

`pheno-tracing` is a **library crate** — it ships no binary, HTTP server, or
network listener. Its attack surface is therefore limited to:

1. Code that callers compile into their own binaries.
2. Data that flows through the `TracePort` API at runtime.

---

## Trust Boundaries

```
┌───────────────────────────────────────────────┐
│  Caller process (trusted)                      │
│                                                │
│  application code ──► TracePort::submit()      │
│                              │                 │
│                    ┌─────────▼──────────┐      │
│                    │  pheno-tracing     │      │
│                    │  (this crate)      │      │
│                    └─────────┬──────────┘      │
│                              │                 │
│                  adapter boundary (trust drop) │
│                              │                 │
│            ┌─────────────────▼──────────────┐  │
│            │  Backend adapter               │  │
│            │  (InMemory / Stdout / OTLP)    │  │
│            └────────────────────────────────┘  │
└───────────────────────────────────────────────┘
                              │
                   network boundary (OTLP only)
                              │
                    ┌─────────▼─────────┐
                    │  OTLP collector   │
                    │  (untrusted net)  │
                    └───────────────────┘
```

**Trust drops at the adapter boundary.** The crate itself performs no
authentication of its callers; all callers share the same process trust level.

---

## Data Sensitivity

`TraceOperation` carries:

| Field | Sensitivity | Notes |
|---|---|---|
| `trace_id` / `span_id` | Low | Opaque identifiers; no PII by design. |
| `name` | Medium | Operation names may leak internal service topology. |
| `attributes` | High | Callers may accidentally include PII (user IDs, request paths, auth tokens). |

**Mitigation:** The crate makes no assumption about attribute content. Consumers
are responsible for scrubbing PII before calling `submit()`. A future
`CarefulAdapter` wrapper may add an attribute allowlist; that is out of scope
for this crate's core.

---

## Unsafe Code Posture

- **No `unsafe` blocks** exist in `src/`. `cargo geiger` confirms zero unsafe
  usage in first-party code.
- Transitive unsafe via `tokio` / `parking_lot` is expected and accepted; those
  are audited upstream crates.
- Any PR that introduces `unsafe` must include a `// SAFETY:` comment and pass
  `cargo clippy -- -D unsafe_code` (to be added to CI).

---

## Threat Catalog (STRIDE)

| Threat | Category | Risk | Mitigation |
|---|---|---|---|
| Caller injects PII in span attributes | Information disclosure | Medium | Documented as caller responsibility; out of scope for this crate. |
| Lock poisoning via panicking thread | Denial of service | Low | `InMemoryAdapter::submit` recovers via `poisoned.into_inner()` with a `tracing::warn!`. |
| OTLP exporter sends spans to wrong collector | Spoofing | Medium | Adapter configuration is caller-controlled; TLS is exporter-specific. |
| Cardinality explosion fills process memory | DoS | Medium | `CardinalityCap` in `src/cardinality.rs` bounds label count; documented in README §Cardinality. |
| Malicious span name causes regex ReDoS | DoS | Low | No regex is applied to span names inside this crate. |
| Supply-chain compromise via dep update | Tampering | Medium | `cargo deny` + `cargo audit` + `deny.toml` pin advisory DB; Renovate keeps deps current. |

---

## Backend Export Assumptions

- `InMemoryAdapter` — no network; safe for tests and offline use.
- `StdoutAdapter` — writes to process stdout; sensitive in shared log
  aggregation environments where stdout is forwarded to a log service.
- Future OTLP adapter — must validate `OTEL_EXPORTER_OTLP_ENDPOINT` is a
  trusted endpoint before sending; TLS should be required for production use.

---

## Attack Surface Summary

| Surface | Exposure | Notes |
|---|---|---|
| `TracePort::submit()` | In-process only | No network, no deserialization of untrusted input at this layer. |
| `TracePort::flush()` | In-process only | Returns `TraceError`; callers handle. |
| `Sampler::should_sample()` | In-process only | Pure computation; no I/O. |
| `CardinalityCap` | In-process only | Mutex-protected counter; bounded. |
| `Cargo.toml` dependencies | Supply chain | Mitigated by `cargo deny` + `cargo audit`. |

---

## Out of Scope

- Authentication/authorization of span data (no auth surface in a tracing lib).
- Cryptographic signing of spans (declared non-cryptographic in `SECURITY.md`).
- Tenant isolation (per-process; no multi-tenant model).
