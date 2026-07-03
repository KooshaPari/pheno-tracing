use pheno_tracing::adapters::InMemoryAdapter;
use pheno_tracing::port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceStatus};
use std::collections::HashMap;

fn op(trace: &str, span: &str) -> TraceOperation {
    TraceOperation {
        trace_id: TraceId(trace.into()),
        span_id: SpanId(span.into()),
        parent_span_id: None,
        kind: SpanKind::Internal,
        name: "op".into(),
        attributes: HashMap::new(),
    }
}

/// Poison `adapter.spans` by panicking a thread while it holds the lock.
/// Exercises the `Err(poisoned) => poisoned.into_inner()` recovery branch in
/// `InMemoryAdapter::submit`.
fn poison_lock(adapter: &InMemoryAdapter) {
    let spans = adapter.spans.clone();
    std::thread::spawn(move || {
        let _guard = spans.lock().unwrap();
        panic!("intentional poison");
    })
    .join()
    .expect_err("poisoning thread must panic");
    assert!(adapter.spans.is_poisoned());
}

#[tokio::test]
async fn test_in_memory_adapter_submits_span() {
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
    assert_eq!(result.trace_id.0, "trace-001");
    assert_eq!(result.span_id.0, "span-001");
    assert_eq!(result.status, TraceStatus::Ok);
    let spans = adapter.spans.lock().unwrap();
    assert_eq!(spans.len(), 1);
}

#[tokio::test]
async fn test_in_memory_adapter_records_attributes() {
    let adapter = InMemoryAdapter::new();
    let op = TraceOperation {
        trace_id: TraceId("trace-attrs".into()),
        span_id: SpanId("span-attrs".into()),
        parent_span_id: None,
        kind: SpanKind::Producer,
        name: "publish-event".into(),
        attributes: HashMap::from([
            ("messaging.system".to_string(), "kafka".to_string()),
            ("messaging.destination".to_string(), "events".to_string()),
        ]),
    };
    let result = adapter.submit(op).await;
    assert_eq!(result.status, TraceStatus::Ok);
    let spans = adapter.spans.lock().unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].attributes.get("messaging.system").unwrap(),
        "kafka"
    );
    assert_eq!(
        spans[0].attributes.get("messaging.destination").unwrap(),
        "events"
    );
}

#[tokio::test]
async fn test_in_memory_adapter_flush() {
    let adapter = InMemoryAdapter::new();
    let result = adapter.flush().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_in_memory_adapter_parent_child_relationship() {
    let adapter = InMemoryAdapter::new();
    let parent = TraceOperation {
        trace_id: TraceId("trace-tree".into()),
        span_id: SpanId("span-root".into()),
        parent_span_id: None,
        kind: SpanKind::Internal,
        name: "root".into(),
        attributes: HashMap::new(),
    };
    let child = TraceOperation {
        trace_id: TraceId("trace-tree".into()),
        span_id: SpanId("span-child".into()),
        parent_span_id: Some(SpanId("span-root".into())),
        kind: SpanKind::Internal,
        name: "child".into(),
        attributes: HashMap::new(),
    };
    adapter.submit(parent).await;
    adapter.submit(child).await;
    let spans = adapter.spans.lock().unwrap();
    assert_eq!(spans.len(), 2);
    assert!(spans[1].parent_span_id.is_some());
    assert_eq!(spans[1].parent_span_id.as_ref().unwrap().0, "span-root");
}

#[tokio::test]
async fn test_in_memory_adapter_submit_recovers_from_poisoned_lock() {
    let adapter = InMemoryAdapter::new();
    poison_lock(&adapter);

    // submit must succeed despite the poisoned mutex — the recovery branch
    // takes `poisoned.into_inner()` instead of propagating the panic.
    let result = adapter.submit(op("trace-poison", "span-poison")).await;
    assert_eq!(result.status, TraceStatus::Ok);
    assert_eq!(result.trace_id.0, "trace-poison");

    let spans = adapter.spans.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].span_id.0, "span-poison");
}

#[tokio::test]
async fn test_in_memory_adapter_poison_recovery_preserves_prior_spans() {
    let adapter = InMemoryAdapter::new();
    adapter.submit(op("trace-pre", "span-pre")).await;

    poison_lock(&adapter);

    // Recovery uses `into_inner()`, so the span buffered before poisoning must
    // survive alongside the one submitted after.
    adapter.submit(op("trace-post", "span-post")).await;

    let spans = adapter.spans.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].span_id.0, "span-pre");
    assert_eq!(spans[1].span_id.0, "span-post");
}
