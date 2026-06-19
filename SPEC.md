# pheno-tracing — SPEC.md

**Date:** 2026-06-18
**Status:** ACTIVE
**ADR:** ADR-036 (canonical tracing substrate for the pheno-* fleet)
**Drift:** L5-110 Drift 1 (CRITICAL) — promotion from scattered `./crates/pheno-tracing` duplicates to a single top-level substrate repo
**Substrate type:** `pheno-*-lib` (per ADR-023)

---

## 1. Purpose

`pheno-tracing` is the **canonical port-driven distributed tracing substrate** for the pheno-* fleet. It defines the stable `TracePort` trait that every adapter implements, plus the canonical `TraceId` / `SpanId` / `TraceOperation` / `TraceResult` types that flow across the fleet.

It is the *port+adapter* layer that sits below the `tracing` ecosystem (`tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `pheno-otel`) and the OTLP export pipeline.

It is **not** a replacement for `tracing` itself — it is the fleet-wide **contract** that wraps `tracing` so backend swaps (in-memory, stdout, OTLP, Jaeger, Honeycomb) don't ripple through the call graph.

## 2. Public API (v0.1.0)

```rust
// Re-exports at crate root
pub use port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceResult};

// Port layer (src/port.rs)
pub struct TraceId(pub String);
pub struct SpanId(pub String);
pub enum SpanKind { Internal, Client, Server, Producer, Consumer }
pub struct TraceOperation {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub kind: SpanKind,
    pub name: String,
    pub attributes: HashMap<String, String>,
}
pub struct TraceResult {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub status: TraceStatus,
}
pub enum TraceStatus { Ok, Error(String) }
#[async_trait::async_trait]
pub trait TracePort: Send + Sync {
    async fn submit(&self, op: TraceOperation) -> TraceResult;
    async fn flush(&self) -> Result<(), String>;
}

// Adapters (src/adapters.rs)
pub struct InMemoryAdapter { pub spans: Arc<Mutex<Vec<TraceOperation>>> }
impl TracePort for InMemoryAdapter { /* spans stored in Arc<Mutex<Vec<...>>> */ }

pub struct StdoutAdapter;
impl TracePort for StdoutAdapter { /* prints to stdout */ }
```

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Consumer (pheno-errors, pheno-context, pheno-config, etc.) │
│   depends on pheno-tracing for span submission              │
└────────────────────────┬────────────────────────────────────┘
                         │  TracePort::submit(TraceOperation)
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ pheno-tracing (this crate)                                  │
│   - TracePort trait                                         │
│   - TraceOperation / TraceResult / TraceId / SpanId types   │
│   - InMemoryAdapter (tests)                                 │
│   - StdoutAdapter (local debug)                             │
│   - (future) OtlpAdapter, JaegerAdapter, HoneycombAdapter   │
└────────────────────────┬────────────────────────────────────┘
                         │  std::any / trait object
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ tracing + tracing-subscriber + tracing-opentelemetry        │
│   OTLP export → Jaeger / Honeycomb / Tempo / OTel Collector │
└─────────────────────────────────────────────────────────────┘
```

## 4. Non-goals

- `pheno-tracing` is **not** a replacement for the `tracing` crate. It is the port layer above it.
- `pheno-tracing` is **not** the OTLP exporter. That is `pheno-otel` and the `tracing-opentelemetry` adapter.
- `pheno-tracing` is **not** a metrics substrate. That is `pheno-otel` / `Prometheus` / `OpenMetrics`.
- `pheno-tracing` is **not** a logging substrate. It exposes a tracing-port contract; logging lives in the standard `tracing` macros.

## 5. When to use

- You are building a pheno-* crate and need to submit spans.
- You need a stable port trait so backend swaps (in-memory / OTLP / Jaeger) don't ripple.
- You need an in-memory adapter for testing span submission without a real backend.
- You need fleet-wide observation through the same port contract.

## 6. When NOT to use

- You need OTLP wire-format export → use `pheno-otel` + `tracing-opentelemetry`.
- You need metrics / counters / gauges → use `pheno-otel` or `Prometheus`.
- You need raw log output → use `tracing` directly.
- You need OpenTelemetry **resources** (service.name, service.version) → use `pheno-otel` `Resource::builder()`.

## 7. Migration story

- **Before this PR:** `pheno-tracing` was scattered across 5 duplicates:
  - `./crates/pheno-tracing/` (in `KooshaPari/pheno`)
  - `./FocalPoint/crates/pheno-tracing/`
  - `./FocalPoint/pheno-tracing/`
  - `./PhenoCompose/packages/pheno-tracing/` (TypeScript mirror, out of scope)
  - `/private/tmp/t15-batch-output/rust/pheno-tracing/` (T15.5 batch output)
- **After this PR:** `KooshaPari/pheno-tracing` is the **single canonical top-level substrate repo**. All scattered copies will be removed in follow-up PRs (see `KooshaPari/phenotype-registry#<TBD>` + `KooshaPari/pheno#<TBD>`).

## 8. Quality bar (ADR-023 Rule 3.1)

- **Spec** — this file (1 page, comprehensive).
- **Docs** — `README.md` + `llms.txt` + `AGENTS.md`.
- **Test matrix** — 8 tests across `tests/adapter_tests.rs` (3) + `tests/port_integration.rs` (4) + 1 doctest in `lib.rs`.
- **Observability** — `tracing-subscriber` JSON formatter, OTLP-ready via `tracing-opentelemetry` consumer.
- **Coverage gate** — 80% lib (ADR-023 Rule 3.1).
- **CI gate** — `.github/workflows/ci.yml` from `pheno-ci-templates` (test + clippy + fmt + coverage + audit + deny + OTLP smoke).
- **Worklog v2.1** — `WORKLOG.md` follows `pheno-worklog-schema` v2.1 (ADR-015 + ADR-025 + ADR-030).

## 9. Cross-references

- ADR-023 (substrate placement, "no random phenoShared")
- ADR-036 (canonical tracing substrate)
- ADR-019 (substrate governance)
- ADR-012 (`tracing` canonical across pheno-* repos)
- ADR-015 / ADR-025 / ADR-030 (worklog v2.1 schema, 11 columns including `device:`)
- L5-110 Drift 1 (CRITICAL — promotion of scattered duplicates to top-level repo)
- T15.5 v9 plan track (this PR)
- `phenotype-registry` row `lib-pheno-tracing` (registered this turn)
- `phenotype-registry` row `lib-pheno-capacity` (registered this turn, L5-110.x)
