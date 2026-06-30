//! Port layer for tracing operations.
//!
//! The `TracePort` trait is the fleet-wide contract for submitting spans.
//! Adapters (in-memory, stdout, OTLP, etc.) implement this trait; consumers
//! depend only on the port so backend swaps don't ripple through the call
//! graph.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Unique trace identifier (128-bit, base16-encoded in OTLP).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

/// Unique span identifier (64-bit, base16-encoded in OTLP).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub String);

/// Kind of span (matches OpenTelemetry span kinds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    /// Internal operation within the application.
    Internal,
    /// Outbound request to an external service.
    Client,
    /// Inbound request from an external caller.
    Server,
    /// Message produced to a queue or stream.
    Producer,
    /// Message consumed from a queue or stream.
    Consumer,
}

/// Single trace/span operation submitted to a [`TracePort`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceOperation {
    /// 128-bit trace identifier (base16-encoded).
    pub trace_id: TraceId,
    /// 64-bit span identifier (base16-encoded).
    pub span_id: SpanId,
    /// Optional parent span identifier for building trace trees.
    pub parent_span_id: Option<SpanId>,
    /// Classification of the span (client, server, internal, etc).
    pub kind: SpanKind,
    /// Short human-readable name for the operation.
    pub name: String,
    /// Key-value attributes attached to the span.
    pub attributes: HashMap<String, String>,
}

/// Result of a trace submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    /// Trace identifier from the submitted operation.
    pub trace_id: TraceId,
    /// Span identifier from the submitted operation.
    pub span_id: SpanId,
    /// Outcome status (Ok or Error with message).
    pub status: TraceStatus,
}

/// Status of a trace operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    /// Span was accepted successfully.
    Ok,
    /// Span submission failed with the given error description.
    Error(String),
}

/// Typed error for trace port operations (L14 audit fix).
///
/// Replaces bare `Result<(), String>` in port/adapter paths so callers have
/// structured categories to match on and adapters can attach recovery hints.
#[derive(Debug, Error)]
pub enum TraceError {
    /// The underlying buffer or mutex was poisoned by a panicking thread.
    #[error("trace buffer poisoned: {0}")]
    BufferPoisoned(String),

    /// A flush operation failed, e.g. the OTLP exporter returned an error.
    #[error("flush failed: {0}")]
    FlushFailed(String),

    /// Cardinality cap exceeded; the span was dropped.
    #[error("cardinality cap exceeded (limit={limit}, current={current})")]
    CardinalityCapExceeded {
        /// Configured cardinality cap.
        limit: usize,
        /// Observed cardinality at the time of rejection.
        current: usize,
    },

    /// Backend export error (e.g. network failure when forwarding to OTLP).
    #[error("backend export error: {0}")]
    BackendExport(String),
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
    async fn flush(&self) -> Result<(), TraceError>;
}
