# Runbook — pheno-tracing

_Audit fix for L27 (v37 scorecard). Covers expected failure modes, SLO-lite
expectations, and incident response for tracing/export regressions._

## SLO-Lite

`pheno-tracing` is a library, not a service — it has no uptime SLA. The
SLO-lite below applies to **consumers** that embed pheno-tracing:

| Signal | Target | Alert threshold |
|---|---|---|
| Span submit error rate | < 0.1% of spans | > 0.5% over 5 min |
| Flush duration (p99) | < 100 ms | > 500 ms |
| Lock-poison events | 0 per day | Any occurrence |
| Cardinality cap hits | < 1% of spans | > 5% over 5 min |

---

## Known Failure Modes

### 1. Lock Poisoning (`InMemoryAdapter`)

**Symptom:** `tracing::warn!` log line containing `"mutex lock poisoned"` in
`pheno_tracing.in_memory` target.

**Cause:** A thread panicked while holding the spans mutex. The adapter
recovers by calling `into_inner()` on the poisoned guard; span data is not
lost, but the panicking thread's stack may indicate a deeper bug.

**Response:**
1. Check process logs for a preceding panic with `target = "pheno_tracing"`.
2. Identify the panicking call site.
3. Fix the root-cause panic; the lock-poison event is a symptom, not the bug.

---

### 2. `flush()` Returns `TraceError::FlushFailed`

**Symptom:** Caller's `flush().await` returns `Err(TraceError::FlushFailed(_))`.

**Cause (OTLP adapter):** Network error, collector unreachable, or TLS
handshake failure.

**Response:**
1. Check `OTEL_EXPORTER_OTLP_ENDPOINT` env var is correct and reachable.
2. Verify collector TLS cert is valid if using `https://`.
3. Retry with backoff; spans buffered in the exporter may be lost on repeated
   flush failures.

---

### 3. Cardinality Cap Exceeded

**Symptom:** `TraceError::CardinalityCapExceeded { limit, current }` returned
from a future OTLP adapter, or silent span drops in `src/cardinality.rs`.

**Cause:** Too many unique label combinations; often caused by high-cardinality
attributes like user IDs or request IDs being added as span tags.

**Response:**
1. Identify the high-cardinality attribute via `current` count in the error.
2. Move high-cardinality values into span *events* (not attributes) or drop
   them before calling `submit()`.
3. Increase `CardinalityCap` limit if the cardinality is genuinely required.

---

### 4. `cargo build` Breaks After Dependency Bump

**Symptom:** CI fails on `cargo build` after a Renovate PR merges.

**Response:**
1. Run `cargo check` locally.
2. Check if `thiserror`, `async-trait`, or `tracing` introduced a breaking
   API change.
3. Update the affected call sites; pin to a prior minor if needed while
   upstream is fixed.

---

### 5. Bench Regression (future criterion CI gate)

**Symptom:** `cargo bench` shows >20% regression in `rate_limit_consume` or
`tail_based_observe` vs baseline.

**Response:**
1. Run `cargo bench -- --save-baseline before` before the suspect commit.
2. Apply the commit, then `cargo bench -- --baseline before`.
3. Identify the regressing function and profile with `cargo flamegraph` or
   `perf`.

---

## Dependency Audit Response

Run on any `cargo audit` alert:

```bash
cargo audit
# For an advisory to review:
cargo audit --ignore RUSTSEC-YYYY-NNNN  # only after manual triage
```

Check `deny.toml` for existing advisory suppressions; add new ones only with
a comment citing the triage rationale.

---

## PhenoObservability Consolidation Note

`pheno-tracing` has been identified as a candidate for consolidation with the
`PhenoObservability` repo as the org observability kernel. If that merge
proceeds, this runbook should be absorbed into `PhenoObservability`'s ops
documentation and updated to cover the unified exporter pipeline.
