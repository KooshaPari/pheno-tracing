# pheno-tracing

> Canonical port-driven distributed tracing substrate for the pheno-* fleet (ADR-036).
> One-line `TracePort` trait; every fleet crate that submits spans depends on this for
> fleet-wide observation and swappable backends (in-memory, stdout, OTLP, Jaeger, Honeycomb).

## Quickstart

```toml
# Cargo.toml
[dependencies]
pheno-tracing = "0.1"
```

```rust
use pheno_tracing::adapters::InMemoryAdapter;
use pheno_tracing::port::{TraceId, SpanId, TraceOperation, SpanKind, TracePort};
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let adapter = InMemoryAdapter::new();
    let op = TraceOperation {
        trace_id: TraceId("trace-001".into()),
        span_id: SpanId("span-001".into()),
        parent_span_id: None,
        kind: SpanKind::Internal,
        name: "test-span".into(),
        attributes: HashMap::new(),
    };
    let result = adapter.submit(op).await;
    assert_eq!(result.status, pheno_tracing::port::TraceStatus::Ok);
}
```

## When to use

- You are building a pheno-* crate and need to submit spans.
- You need a stable port trait so backend swaps (in-memory / OTLP / Jaeger) don't ripple.
- You need an in-memory adapter for testing span submission without a real backend.
- You need fleet-wide observation through the same port contract.

## When NOT to use

- You need OTLP wire-format export → use [`pheno-otel`](https://github.com/KooshaPari/pheno-otel) + `tracing-opentelemetry`.
- You need metrics / counters / gauges → use `pheno-otel` or `Prometheus`.
- You need raw log output → use the `tracing` crate directly.
- You need OpenTelemetry **resources** (service.name, service.version) → use `pheno-otel` `Resource::builder()`.

## Architecture

```
Consumer (pheno-errors, pheno-context, pheno-config, etc.)
   depends on pheno-tracing for span submission
                         │
                         ▼  TracePort::submit(TraceOperation)
                  ┌──────────────────────┐
                  │   pheno-tracing      │   (this crate)
                  │   - TracePort trait  │
                  │   - InMemoryAdapter  │
                  │   - StdoutAdapter    │
                  └──────────┬───────────┘
                             │
                             ▼
                  ┌──────────────────────┐
                  │ tracing + tracing-   │
                  │ subscriber + tracing-│
                  │ opentelemetry        │
                  └──────────┬───────────┘
                             │  OTLP
                             ▼
                  Jaeger / Honeycomb / Tempo / OTel Collector
```

## See also

- [`SPEC.md`](./SPEC.md) — full specification (1 page).
- [`AGENTS.md`](./AGENTS.md) — agent constitution (build/test/conventions).
- [`CHANGELOG.md`](./CHANGELOG.md) — release notes.
- [`WORKLOG.md`](./WORKLOG.md) — change history (v2.1 schema).
- [`LICENSE-MIT`](./LICENSE-MIT) / [`LICENSE-APACHE`](./LICENSE-APACHE) — dual license.
- [`llms.txt`](./llms.txt) — curated LLM-readable file index.
- [`pheno-otel`](https://github.com/KooshaPari/pheno-otel) — sibling OTLP substrate.
- ADR-036 — canonical tracing substrate decision.
- ADR-023 — substrate placement policy ("no random phenoShared").
- L5-110 Drift 1 — promotion of scattered duplicates to top-level repo.

## License

Dual-licensed under MIT or Apache-2.0, at your option. See [`LICENSE-MIT`](./LICENSE-MIT) and [`LICENSE-APACHE`](./LICENSE-APACHE).
