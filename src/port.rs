//! Port layer for tracing operations.
//!
//! The `TracePort` trait is the fleet-wide contract for submitting spans.
//! Adapters (in-memory, stdout, OTLP, etc.) implement this trait; consumers
//! depend only on the port so backend swaps don't ripple through the call
//! graph.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique trace identifier (128-bit, base16-encoded in OTLP).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

/// Unique span identifier (64-bit, base16-encoded in OTLP).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub String);

/// Kind of span (matches OpenTelemetry span kinds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    /// In-process operation with no remote side.
    Internal,
    /// Outbound call to a remote service.
    Client,
    /// Inbound request from a remote caller.
    Server,
    /// Message published to a broker or queue.
    Producer,
    /// Message consumed from a broker or queue.
    Consumer,
}

/// Single trace/span operation submitted to a [`TracePort`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceOperation {
    /// Trace identifier shared by all spans in the request tree.
    pub trace_id: TraceId,
    /// Identifier of this span within the trace.
    pub span_id: SpanId,
    /// Parent span identifier, if any.
    pub parent_span_id: Option<SpanId>,
    /// OpenTelemetry span kind.
    pub kind: SpanKind,
    /// Human-readable span name.
    pub name: String,
    /// String key/value attributes attached to the span.
    pub attributes: HashMap<String, String>,
}

/// Result of a trace submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    /// Trace identifier echoed from the submitted operation.
    pub trace_id: TraceId,
    /// Span identifier echoed from the submitted operation.
    pub span_id: SpanId,
    /// Outcome of the submission.
    pub status: TraceStatus,
}

/// Status of a trace operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    /// Span accepted successfully.
    Ok,
    /// Span rejected or failed with a message.
    Error(String),
}

/// Port trait for tracing backends.
///
/// Every adapter (in-memory, stdout, OTLP, Jaeger, Honeycomb, etc.) implements
/// this trait. Consumers depend only on the port so backend swaps are local.
#[async_trait::async_trait]
pub trait TracePort: Send + Sync {
    /// Submit a single span. Returns the result (status + IDs) to the caller.
    async fn submit(&self, op: TraceOperation) -> TraceResult;

    /// Flush any buffered spans. Adapters that buffer (e.g. OTLP batch) should
    /// ensure the next call to `submit` happens after a clean flush.
    async fn flush(&self) -> Result<(), String>;
}
