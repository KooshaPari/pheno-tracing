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
    Internal,
    Client,
    Server,
    Producer,
    Consumer,
}

/// Single trace/span operation submitted to a [`TracePort`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceOperation {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub kind: SpanKind,
    pub name: String,
    pub attributes: HashMap<String, String>,
}

/// Result of a trace submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceResult {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub status: TraceStatus,
}

/// Status of a trace operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceStatus {
    Ok,
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

// ---------------------------------------------------------------------------
// proptest::Arbitrary impls (v20-T5 / L23)
//
// Only implemented for the **simple** wire types — `TraceId`, `SpanId`,
// `SpanKind`, `TraceStatus`. `TraceOperation` and `TraceResult` carry
// `HashMap<String, String>` attributes that we deliberately do NOT model
// here (the attribute-cardinality cap is exercised by integration tests in
// `tests/cardinality_cap.rs`). Keeping the `Arbitrary` impls small keeps
// the generated search space bounded and the CI runtime tight.
// ---------------------------------------------------------------------------

impl proptest::arbitrary::Arbitrary for TraceId {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        // OTLP W3C-compatible: 32 lowercase hex chars.
        proptest::string::string_regex("[0-9a-f]{32}")
            .expect("trace_id regex")
            .prop_map(TraceId)
            .boxed()
    }
}

impl proptest::arbitrary::Arbitrary for SpanId {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        // OTLP W3C-compatible: 16 lowercase hex chars.
        proptest::string::string_regex("[0-9a-f]{16}")
            .expect("span_id regex")
            .prop_map(SpanId)
            .boxed()
    }
}

impl proptest::arbitrary::Arbitrary for SpanKind {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        proptest::prop_oneof![
            proptest::strategy::Just(SpanKind::Internal),
            proptest::strategy::Just(SpanKind::Client),
            proptest::strategy::Just(SpanKind::Server),
            proptest::strategy::Just(SpanKind::Producer),
            proptest::strategy::Just(SpanKind::Consumer),
        ]
        .boxed()
    }
}

impl proptest::arbitrary::Arbitrary for TraceStatus {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy;
        proptest::prop_oneof![
            proptest::strategy::Just(TraceStatus::Ok),
            proptest::string::string_regex("[A-Za-z0-9 _\\-\\.]{1,80}")
                .expect("trace error regex")
                .prop_map(TraceStatus::Error)
                .boxed(),
        ]
        .boxed()
    }
}
