//! v20-T5 (L23) — proptest smoke test for `pheno-tracing`.
//!
//! Property: the OTLP-spec-shaped `TraceId` / `SpanId` always
//! round-trip through `Debug`; `TraceStatus` always renders a
//! non-empty `Display`; `TraceOperation` always carries a non-empty
//! `name`.
//!
//! Run with:
//!
//! ```bash
//! cargo test --test proptest_smoke
//! ```

use proptest::prelude::*;

use pheno_tracing::port::{SpanId, TraceId, TraceOperation, TraceStatus};

proptest! {
    /// `TraceStatus::Display` output is always non-empty.
    #[test]
    fn trace_status_display_is_nonempty(status in any::<TraceStatus>()) {
        let s = format!("{}", status);
        prop_assert!(!s.is_empty(), "Display output for {:?} was empty", status);
    }

    /// `TraceId` and `SpanId` always render via `Debug` with the
    /// expected hex-string shape (32 / 16 chars respectively, per
    /// OTLP spec). The `Arbitrary` impl in `src/port.rs` constrains
    /// both via regex, so this is a regression guard against the
    /// regexes drifting out of OTLP-spec.
    #[test]
    fn trace_id_is_32_hex(id in any::<TraceId>()) {
        let s = id.0.as_str();
        prop_assert_eq!(s.len(), 32, "TraceId should be 32 chars, got {s}");
        prop_assert!(s.chars().all(|c| c.is_ascii_hexdigit()), "TraceId should be hex: {s}");
    }

    #[test]
    fn span_id_is_16_hex(id in any::<SpanId>()) {
        let s = id.0.as_str();
        prop_assert_eq!(s.len(), 16, "SpanId should be 16 chars, got {s}");
        prop_assert!(s.chars().all(|c| c.is_ascii_hexdigit()), "SpanId should be hex: {s}");
    }

    /// `TraceOperation::name` is always non-empty (the impl
    /// constrains via regex `[a-z][a-z0-9_.]{1,48}`).
    #[test]
    fn trace_operation_name_is_nonempty(op in any::<TraceOperation>()) {
        prop_assert!(!op.name.is_empty(), "operation name must be non-empty: {:?}", op);
    }
}