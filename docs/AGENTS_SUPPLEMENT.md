# pheno-tracing — Project Structure & AGENTS Supplement

> **pheno-tracing** is the **canonical distributed-tracing substrate** for the `pheno-*` family (ADR-012, ADR-036B). Every `pheno-*` crate that wants to emit OTLP spans or correlate work-package traces pulls in `pheno-tracing`.

## Status (2026-06-20)

| Pillar | Score | Target |
|---|---|---|
| L60 Structured logging | 2 | 3 (this PR) |
| L61 Metrics | 2 | 2 (deferred) |
| L62 Distributed tracing | **3** | 3 (held) |
| L63 Health/readiness probes | 1 | 2 (P1) |
| L56 OTel collector config | 2 | 3 (this PR) |
| **Net** | **2.0** | **2.6** |

## What this PR adds

1. **`OTEL_COLLECTOR_GRPC_ENDPOINT`** env-var override in `init_otlp()` (with safe default `http://localhost:4317`).
2. **`OTEL_SERVICE_NAME`** env-var override (replaces compile-time `CARGO_PKG_NAME`).
3. **`init_otlp_from_env()`** helper — single-call bootstrap for apps that prefer env-var configuration.
4. **`tests/otlp_env.rs`** — 4 new integration tests covering env override, default fallback, invalid endpoint error path, and service-name propagation.
5. **3 example configs** under `examples/otel-collector/` — `collector.yaml` (basic gRPC), `collector-tls.yaml` (mTLS), `collector-multi-receiver.yaml` (multi-OTLP).

## Test plan

- [x] `cargo test -p pheno-tracing` passes
- [x] `OTEL_COLLECTOR_GRPC_ENDPOINT=http://localhost:14317 RUST_LOG=debug cargo run --example otlp_demo` connects & sends spans
- [x] `cargo doc -p pheno-tracing --no-deps` builds clean

## Migration (for sibling `pheno-*` crates)

```rust
// before
pheno_tracing::init_otlp("pheno-flags")?;

// after (opt-in env override)
pheno_tracing::init_otlp_from_env()?;
```

## Refs

- ADR-012 (pheno-tracing canonical across pheno-\* repos)
- ADR-036B (pheno-tracing substrate canonical — re-affirmed 2026-06-18)
- 71-pillar gap: L56 OTel collector config (2→3)
