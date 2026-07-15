//! v20-T5 (L23) — proptest property test for `pheno-tracing`.
//!
//! Properties verified:
//!
//! 1. `TraceId` round-trips through serde JSON (any 32-char hex id
//!    survives serialize → deserialize).
//! 2. `SpanId` round-trips through serde JSON.
//! 3. `SpanKind` round-trips through serde JSON (one of the five
//!    documented OTel kinds).
//! 4. `TraceStatus` round-trips through serde JSON (either `Ok` or
//!    `Error(...)`).
//!
//! Run with:
//!
//! ```bash
//! cargo test --test proptest_arbitrary
//! ```

use proptest::prelude::*;

use pheno_tracing::{SpanId, SpanKind, TraceId, TraceStatus};

proptest! {
    /// `TraceId` round-trips through serde JSON without loss. The
    /// generated ids are constrained to 32 lowercase hex chars
    /// (OTLP W3C Trace Context §3.2.2.2) so the round-trip is exact.
    #[test]
    fn trace_id_serde_roundtrip(id in any::<TraceId>()) {
        let json = serde_json::to_string(&id).expect("serialize TraceId");
        let back: TraceId = serde_json::from_str(&json).expect("deserialize TraceId");
        prop_assert_eq!(id, back);
    }

    /// `SpanId` round-trips through serde JSON. Generated ids are
    /// 16 lowercase hex chars (OTLP §3.2.2.3).
    #[test]
    fn span_id_serde_roundtrip(id in any::<SpanId>()) {
        let json = serde_json::to_string(&id).expect("serialize SpanId");
        let back: SpanId = serde_json::from_str(&json).expect("deserialize SpanId");
        prop_assert_eq!(id, back);
    }

    /// `SpanKind` round-trips through serde JSON. The five OTLP kinds
    /// (`Internal`, `Client`, `Server`, `Producer`, `Consumer`) are
    /// each emitted in snake_case by serde.
    #[test]
    fn span_kind_serde_roundtrip(kind in any::<SpanKind>()) {
        let json = serde_json::to_string(&kind).expect("serialize SpanKind");
        let back: SpanKind = serde_json::from_str(&json).expect("deserialize SpanKind");
        prop_assert_eq!(kind, back);
    }

    /// `TraceStatus` round-trips through serde JSON. `Ok` serializes
    /// to `"Ok"`; `Error(s)` serializes to `{"Error": s}`.
    #[test]
    fn trace_status_serde_roundtrip(status in any::<TraceStatus>()) {
        let json = serde_json::to_string(&status).expect("serialize TraceStatus");
        let back: TraceStatus = serde_json::from_str(&json).expect("deserialize TraceStatus");
        prop_assert_eq!(status, back);
    }
}
