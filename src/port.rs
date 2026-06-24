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
    /// Default: work internal to a service, no remote parent.
    Internal,
    /// Outbound RPC / HTTP / database call.
    Client,
    /// Inbound RPC / HTTP / queue handler.
    Server,
    /// Message published to a broker (Kafka, NATS, etc.).
    Producer,
    /// Message consumed from a broker.
    Consumer,
}

/// Single trace/span operation submitted to a [`TracePort`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceOperation {
    /// Identifier of the trace this span belongs to.
    pub trace_id: TraceId,
    /// Identifier of this span (unique within the trace).
    pub span_id: SpanId,
    /// Parent span id, if this is a child of another span.
    pub parent_span_id: Option<SpanId>,
    /// Span kind (client, server, internal, ...).
    pub kind: SpanKind,
    /// Human-readable span name (e.g. `"GET /users/:id"`).
    pub name: String,
    /// Free-form key/value attributes attached to the span.
    pub attributes: HashMap<String, String>,
}

/// Result of a trace submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    /// Echo of the trace id the backend accepted.
    pub trace_id: TraceId,
    /// Echo of the span id the backend accepted.
    pub span_id: SpanId,
    /// Submission status (Ok or Error with detail).
    pub status: TraceStatus,
}

/// Status of a trace operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    /// Span was accepted by the backend.
    Ok,
    /// Backend rejected or failed to record the span; the string is a
    /// human-readable reason suitable for logs.
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
