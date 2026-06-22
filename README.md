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

## Sampling (v22-T2 / L26)

`pheno-tracing` ships six sampling strategies, each implementing the
`Sampler` port trait. Consumers pick one (or compose them) at startup
from configuration. All six are reachable through `pheno_tracing::*`.

| Strategy               | Use when …                                                                                | Memory    | CPU      | Correctness    |
| :--------------------- | :---------------------------------------------------------------------------------------- | :-------- | :------- | :------------- |
| `AlwaysOnSampler`      | debug builds, pre-prod; you want every span recorded                                      | O(1)      | O(1)     | exact          |
| `AlwaysOffSampler`     | load tests, soak; you want every span dropped                                             | O(1)      | O(1)     | exact          |
| `ProbabilisticSampler` | you want a fraction of traces recorded; no parent contract; OTel-spec default            | O(1)      | O(1)     | probabilistic  |
| `ParentBasedSampler`   | distributed tracing; honor the upstream W3C sampled bit; respect the call graph          | O(1)      | O(1)     | exact          |
| `RateLimitedSampler`   | high-throughput services with a hard per-second ingestion budget; you want smoother load  | O(1)      | O(1)     | statistical    |
| `TailSampler`          | you want error/slow/named capture; rules are explicit; no sliding window state           | O(K)¹     | O(R)²    | exact          |

¹ K = number of unique trace_ids ever observed (set of marked trace_ids).
² R = number of rules; each `observe_outcome` call evaluates all rules.

### Tradeoffs

**Probabilistic vs. rate-limited.** A pure `ProbabilisticSampler` is
correct but bursty: a probabilistic spike can 2x the average rate for
short windows, and a single high-volume trace can dominate the
sample. A pure `RateLimitSampler` (token bucket) is bounded but
single-traceable: one trace can saturate the bucket while sibling
traces are dropped. The hybrid `RateLimitedSampler` (probabilistic
gate + token-bucket cap) gives the best of both: the gate spreads
load across trace_ids; the bucket enforces a hard ceiling. Pick
`ProbabilisticSampler` for low-throughput services (the gate is
enough); pick `RateLimitedSampler` for high-throughput services with
a known ingestion budget (the bucket is the back-end's contract).

**Tail-based vs. head-based.** All `Always*`, `ProbabilisticSampler`,
`ParentBasedSampler`, and `RateLimitedSampler` are **head-based**: the
decision is made at span-start, no information about the span's
outcome is available. `TailSampler` is **tail-based**: the decision
is informed by the span's eventual outcome (error status, duration,
name). The tradeoff is operational: head-based sampling is local
(no coordination between processes) but cannot capture error bursts
that emerge after the head decision; tail-based sampling can capture
the burst but requires the entire trace to be buffered until the
outcome is known, which doubles memory and adds latency. For
services where errors are the primary signal, use `TailSampler`; for
high-throughput services where the head decision is "good enough",
use `ProbabilisticSampler` or `RateLimitedSampler`.

**`TailBasedSampler` vs. `TailSampler` (sliding window vs. rule
list).** The legacy `TailBasedSampler` uses a sliding error-rate
window: "record when the recent error rate exceeds a threshold".
This is good for "capture the long-tail error spike" but bad for
"capture specific slow endpoints" or "capture specific named
endpoints". The new `TailSampler` (v22-T2) uses a discrete rule list
with three rule shapes (error, slow, named) that compose with
logical OR. Pick `TailSampler` for explicit capture rules; pick
`TailBasedSampler` for "error rate > N%" capture.

**Ratio clamping.** `ProbabilisticSampler::new` (and
`TraceIdRatioBased::new`) clamps the input ratio to `[0.0, 1.0]`.
Out-of-range values are silently clamped so a misconfigured
`PHENO_TRACING_SAMPLE_RATE` env var can never produce undefined
behavior; the worst case is "record everything" or "record
nothing", both of which are easy to detect via observability
dashboards. NaN passes through (Rust's `f64::clamp` does not unwrap
NaN); this is documented in the unit test
`v22_trace_id_ratio_based_clamps_ratio_to_unit_interval` and is the
intended behavior, not a bug.

### Quickstart

```rust
use pheno_tracing::{
    CardinalityCap, ProbabilisticSampler, RateLimitedSampler, Sampler, TailSampler,
    TailSamplingRule,
};
use std::sync::Arc;
use std::collections::HashMap;

// Head-based: 10% of all trace_ids, no parent contract.
let sampler = ProbabilisticSampler::new(0.10);
let decision = sampler.should_sample_with_attrs(
    "trace-001",
    "GET /users",
    &HashMap::new(),
);
assert!(decision.is_record() || matches!(decision, _));

// Hybrid: probabilistic gate + token-bucket cap.
let sampler = RateLimitedSampler::new(0.10, 1000.0); // 10% of traces, capped at 1k/sec

// Tail-based: capture errors and slow spans.
let sampler = TailSampler::new(vec![
    TailSamplingRule::errors(),
    TailSamplingRule::slow(500), // 500ms+
]);

// Cardinality cap: 100 unique values per attribute name.
let cap = Arc::new(CardinalityCap::default());
let mut attrs = HashMap::new();
attrs.insert("user_id".to_string(), "user-001".to_string());
let report = cap.process(&mut attrs);
assert_eq!(report.kept, 1);
```

## Cardinality cap (v22-T2 / L26)

`CardinalityCap` bounds the per-process cardinality of span
attributes at 100 unique values per attribute name (the v22 / L26
default; override with `CardinalityCap::new(max_unique_per_attr)`).
Values past the cap are replaced with the `__other__` overflow
marker; values in the seen-set are passed through verbatim. The
seen-set is a `HashMap<name, HashSet<value>>` whose size is
bounded by `O(num_attribute_names * cap)`.

### Tradeoffs

**First-N-wins vs. LRU.** The cap is **first-N-wins**: the first
`cap` distinct values observed for a given attribute name are the
"winners", and additional values are evicted to the overflow
marker. This is the OTel Collector `attributes/limit` processor's
policy and matches Prometheus `sample_limit`. The alternative
(least-recently-used) gives different winners in different
deployments and is harder to reason about operationally; first-N
is deterministic and reproducible. The downside is that a single
high-cardinality attribute can crowd out other attributes from
the per-process cap; if that happens, the cap should be raised,
not changed to LRU.

**Per-process vs. fleet-wide.** The cap is enforced **per-process**:
two replicas of the same service will each have their own seen-set
and may record different "winners" for the same attribute. A
fleet-wide cap requires a coordinator service (out of scope) or
consistent-hash redistribution of the per-process cap across
instances (a future L26 sub-task). For most production
deployments, the per-process cap is sufficient because the OTLP
back-end (Honeycomb, Tempo, etc.) does its own global cap.

**Hash-bucket memory bound.** The seen-set is a
`HashMap<String, HashSet<String>>` (one bucket per attribute
name). Each bucket is independently bounded by `cap`; once full,
the bucket size stays at `cap` and additional distinct values are
discarded. Total memory is therefore `O(num_attribute_names * cap)`.
For the default `cap = 100` and a typical service with 50 distinct
attribute names, the cap uses ~50 KB of stable state. The
seen-set is not auto-reset; for long-lived processes that want to
bound the seen-set size by time, call `CardinalityCap::reset()`
periodically (e.g. from a daily cron).

**Idempotence.** A value in the seen-set is always passed through,
even after the cap is hit. This means a value that was observed
early in the process lifetime is preserved even if it has not been
seen in a long time — the first-N-winners are sticky. The
`CardinalityCap::reset()` call clears the seen-set; use it if you
want the winners to be re-evaluated.

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
