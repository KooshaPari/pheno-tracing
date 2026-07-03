//! Targeted coverage tests for `StdoutAdapter` (T44 audit).
//!
//! The existing `adapter_tests.rs` covers the basic submit/flush happy paths.
//! This file adds:
//!   - Construction via `Default::default()` (the unit-struct form)
//!   - `Clone` and `Copy` semantics (both derives are present in src)
//!   - `Debug` formatting
//!   - All five `SpanKind` variants through the adapter (each prints a
//!     different `kind=...` token)
//!   - Multi-submit: submit two spans, verify both return `TraceStatus::Ok`
//!   - Flush after submit: flush must succeed even after submitting multiple spans
//!   - Attribute passthrough: result IDs must echo the submitted IDs exactly

use pheno_tracing::adapters::StdoutAdapter;
use pheno_tracing::port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceStatus};
use std::collections::HashMap;

fn minimal_op(trace_id: &str, span_id: &str, kind: SpanKind) -> TraceOperation {
    TraceOperation {
        trace_id: TraceId(trace_id.into()),
        span_id: SpanId(span_id.into()),
        parent_span_id: None,
        kind,
        name: format!("{trace_id}-{span_id}"),
        attributes: HashMap::new(),
    }
}

// ── Construction ──────────────────────────────────────────────────────────────

/// `StdoutAdapter` is a unit struct; `Default::default()` must compile and
/// produce a usable instance (same as writing `StdoutAdapter` directly).
#[test]
fn stdout_adapter_default_construction() {
    let _adapter: StdoutAdapter = StdoutAdapter::default();
}

/// `StdoutAdapter` derives `Copy` — copying must yield an independent value
/// that still satisfies the `TracePort` bound at the type level.
#[test]
fn stdout_adapter_copy_semantics() {
    let a = StdoutAdapter;
    let b = a; // copy (not move — StdoutAdapter: Copy)
    // Both `a` and `b` are still usable after the copy.
    let _ = a;
    let _ = b;
}

/// `StdoutAdapter` derives `Clone` — cloning must produce a second value.
#[test]
fn stdout_adapter_clone_semantics() {
    let a = StdoutAdapter;
    let b = a.clone();
    let _ = b;
}

/// `StdoutAdapter` derives `Debug` — formatting must not panic and must
/// produce a non-empty string.
#[test]
fn stdout_adapter_debug_is_non_empty() {
    let s = format!("{:?}", StdoutAdapter);
    assert!(!s.is_empty(), "Debug output must not be empty");
}

// ── submit — SpanKind coverage ────────────────────────────────────────────────

#[tokio::test]
async fn stdout_adapter_submit_internal_span() {
    let adapter = StdoutAdapter;
    let result = adapter.submit(minimal_op("t-int", "s-int", SpanKind::Internal)).await;
    assert_eq!(result.status, TraceStatus::Ok);
    assert_eq!(result.trace_id.0, "t-int");
    assert_eq!(result.span_id.0, "s-int");
}

#[tokio::test]
async fn stdout_adapter_submit_client_span() {
    let adapter = StdoutAdapter;
    let result = adapter.submit(minimal_op("t-cli", "s-cli", SpanKind::Client)).await;
    assert_eq!(result.status, TraceStatus::Ok);
}

#[tokio::test]
async fn stdout_adapter_submit_server_span() {
    let adapter = StdoutAdapter;
    let result = adapter.submit(minimal_op("t-srv", "s-srv", SpanKind::Server)).await;
    assert_eq!(result.status, TraceStatus::Ok);
}

#[tokio::test]
async fn stdout_adapter_submit_producer_span() {
    let adapter = StdoutAdapter;
    let result = adapter.submit(minimal_op("t-prod", "s-prod", SpanKind::Producer)).await;
    assert_eq!(result.status, TraceStatus::Ok);
}

#[tokio::test]
async fn stdout_adapter_submit_consumer_span() {
    let adapter = StdoutAdapter;
    let result = adapter.submit(minimal_op("t-cons", "s-cons", SpanKind::Consumer)).await;
    assert_eq!(result.status, TraceStatus::Ok);
}

// ── multi-submit ──────────────────────────────────────────────────────────────

/// Submitting two spans in sequence must both succeed. StdoutAdapter is
/// stateless so there is no buffer cap to trip.
#[tokio::test]
async fn stdout_adapter_multi_submit_succeeds() {
    let adapter = StdoutAdapter;
    let r1 = adapter.submit(minimal_op("trace-a", "span-1", SpanKind::Internal)).await;
    let r2 = adapter.submit(minimal_op("trace-a", "span-2", SpanKind::Internal)).await;
    assert_eq!(r1.status, TraceStatus::Ok);
    assert_eq!(r2.status, TraceStatus::Ok);
}

// ── flush after submit ────────────────────────────────────────────────────────

/// Flush must succeed even after one or more submits.
#[tokio::test]
async fn stdout_adapter_flush_after_submit() {
    let adapter = StdoutAdapter;
    adapter.submit(minimal_op("t-flush", "s-flush", SpanKind::Internal)).await;
    let result = adapter.flush().await;
    assert!(result.is_ok(), "flush must not fail; got: {result:?}");
}

// ── result ID echo ────────────────────────────────────────────────────────────

/// The returned `TraceResult` must echo the submitted `TraceId` and `SpanId`
/// exactly — adapters must not generate new IDs or truncate them.
#[tokio::test]
async fn stdout_adapter_result_ids_echo_submitted_ids() {
    let adapter = StdoutAdapter;
    let trace_id = "exact-trace-id-0000";
    let span_id = "exact-span-id-0001";
    let op = TraceOperation {
        trace_id: TraceId(trace_id.into()),
        span_id: SpanId(span_id.into()),
        parent_span_id: Some(SpanId("parent-span-id-9999".into())),
        kind: SpanKind::Server,
        name: "echo-test".into(),
        attributes: HashMap::from([
            ("key1".into(), "value1".into()),
            ("key2".into(), "value2".into()),
        ]),
    };
    let result = adapter.submit(op).await;
    assert_eq!(result.trace_id.0, trace_id, "trace_id must be echoed exactly");
    assert_eq!(result.span_id.0, span_id, "span_id must be echoed exactly");
    assert_eq!(result.status, TraceStatus::Ok);
}
