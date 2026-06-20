use pheno_tracing::adapters::InMemoryAdapter;
use pheno_tracing::port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceStatus};
use std::collections::HashMap;

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
