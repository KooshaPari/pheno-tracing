use pheno_tracing::adapters::StdoutAdapter;
use pheno_tracing::port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceStatus};
use std::collections::HashMap;

#[tokio::test]
async fn test_stdout_adapter_submits_span() {
    let adapter = StdoutAdapter;
    let op = TraceOperation {
        trace_id: TraceId("trace-002".into()),
        span_id: SpanId("span-002".into()),
        parent_span_id: None,
        kind: SpanKind::Client,
        name: "client-span".into(),
        attributes: HashMap::new(),
    };
    let result = adapter.submit(op).await;
    assert_eq!(result.status, TraceStatus::Ok);
    assert_eq!(result.trace_id.0, "trace-002");
    assert_eq!(result.span_id.0, "span-002");
}

#[tokio::test]
async fn test_stdout_adapter_with_parent_span() {
    let adapter = StdoutAdapter;
    let op = TraceOperation {
        trace_id: TraceId("trace-parent".into()),
        span_id: SpanId("span-child".into()),
        parent_span_id: Some(SpanId("span-parent".into())),
        kind: SpanKind::Server,
        name: "child-of-parent".into(),
        attributes: HashMap::from([("http.method".to_string(), "GET".to_string())]),
    };
    let result = adapter.submit(op).await;
    assert_eq!(result.status, TraceStatus::Ok);
}

#[tokio::test]
async fn test_stdout_adapter_flush() {
    let adapter = StdoutAdapter;
    let result = adapter.flush().await;
    assert!(result.is_ok());
}
