//! Targeted coverage tests for `src/error.rs::TraceError` (T44 audit).
//!
//! The T43 remediation added `TraceError::Flush` and `TraceError::LockPoisoned`
//! to `src/error.rs` but no tests exercised the enum. This file closes that
//! gap: Display formatting (the `thiserror` contract), Debug formatting, and
//! `std::error::Error` trait surface.

use pheno_tracing::TraceError;

// =============================================================================
// TraceError::Flush
// =============================================================================

/// Display output must embed the inner message and match the `#[error(...)]`
/// format string `"flush failed: {0}"`.
#[test]
fn trace_error_flush_display_contains_inner_message() {
    let err = TraceError::Flush("OTLP export returned status 503".into());
    let s = err.to_string();
    assert!(
        s.contains("flush failed"),
        "expected 'flush failed' in display; got: {s}"
    );
    assert!(
        s.contains("OTLP export returned status 503"),
        "expected inner message in display; got: {s}"
    );
}

/// `Debug` output must be non-empty and contain the variant name.
#[test]
fn trace_error_flush_debug_contains_variant() {
    let err = TraceError::Flush("network timeout".into());
    let s = format!("{err:?}");
    assert!(
        s.contains("Flush"),
        "expected 'Flush' variant in debug output; got: {s}"
    );
    assert!(
        s.contains("network timeout"),
        "expected inner message in debug output; got: {s}"
    );
}

/// `TraceError` must satisfy `std::error::Error` — verify `source()` returns
/// `None` for leaf variants (no wrapped cause).
#[test]
fn trace_error_flush_is_std_error_with_no_source() {
    use std::error::Error;
    let err = TraceError::Flush("no upstream cause".into());
    assert!(
        err.source().is_none(),
        "Flush is a leaf error; source() must return None"
    );
}

// =============================================================================
// TraceError::LockPoisoned
// =============================================================================

/// Display output for `LockPoisoned` must include the `"lock poisoned"` prefix
/// and the inner message per the `#[error("lock poisoned: {0}")]` format.
#[test]
fn trace_error_lock_poisoned_display_contains_inner_message() {
    let err = TraceError::LockPoisoned("span buffer mutex dropped mid-hold".into());
    let s = err.to_string();
    assert!(
        s.contains("lock poisoned"),
        "expected 'lock poisoned' in display; got: {s}"
    );
    assert!(
        s.contains("span buffer mutex dropped mid-hold"),
        "expected inner message in display; got: {s}"
    );
}

/// `Debug` output must be non-empty and identify the `LockPoisoned` variant.
#[test]
fn trace_error_lock_poisoned_debug_contains_variant() {
    let err = TraceError::LockPoisoned("thread panicked while holding lock".into());
    let s = format!("{err:?}");
    assert!(
        s.contains("LockPoisoned"),
        "expected 'LockPoisoned' in debug output; got: {s}"
    );
}

/// `LockPoisoned` is also a leaf error — `source()` returns `None`.
#[test]
fn trace_error_lock_poisoned_is_std_error_with_no_source() {
    use std::error::Error;
    let err = TraceError::LockPoisoned("poisoned".into());
    assert!(
        err.source().is_none(),
        "LockPoisoned is a leaf error; source() must return None"
    );
}

// =============================================================================
// Variant discrimination
// =============================================================================

/// The two variants must be distinct — pattern matching must not conflate them.
#[test]
fn trace_error_variants_are_distinguishable() {
    let flush = TraceError::Flush("flush err".into());
    let lock = TraceError::LockPoisoned("lock err".into());

    let flush_label = match &flush {
        TraceError::Flush(_) => "flush",
        TraceError::LockPoisoned(_) => "lock",
    };
    let lock_label = match &lock {
        TraceError::Flush(_) => "flush",
        TraceError::LockPoisoned(_) => "lock",
    };

    assert_eq!(flush_label, "flush");
    assert_eq!(lock_label, "lock");
}

/// Both variants must carry their full inner string through the `Display`
/// contract — an empty inner message must not cause a panic or truncation.
#[test]
fn trace_error_empty_inner_string_is_valid() {
    let flush_empty = TraceError::Flush(String::new());
    let lock_empty = TraceError::LockPoisoned(String::new());

    // Must not panic; Display must produce a non-empty string (the prefix alone).
    let s1 = flush_empty.to_string();
    let s2 = lock_empty.to_string();
    assert!(!s1.is_empty(), "Display must not produce empty output for Flush(\"\")");
    assert!(!s2.is_empty(), "Display must not produce empty output for LockPoisoned(\"\")");
}

// =============================================================================
// port::TraceError — the parallel enum in src/port.rs
// =============================================================================
//
// `src/port.rs` defines a *separate* `TraceError` enum (used internally by the
// port/adapter machinery) with four variants: `BufferPoisoned`, `FlushFailed`,
// `CardinalityCapExceeded`, and `BackendExport`. These were also untested prior
// to T44.

use pheno_tracing::port::TraceError as PortTraceError;

#[test]
fn port_trace_error_buffer_poisoned_display() {
    let err = PortTraceError::BufferPoisoned("mutex dropped during span flush".into());
    let s = err.to_string();
    assert!(s.contains("trace buffer poisoned"), "got: {s}");
    assert!(s.contains("mutex dropped during span flush"), "got: {s}");
}

#[test]
fn port_trace_error_flush_failed_display() {
    let err = PortTraceError::FlushFailed("OTLP endpoint unreachable".into());
    let s = err.to_string();
    assert!(s.contains("flush failed"), "got: {s}");
    assert!(s.contains("OTLP endpoint unreachable"), "got: {s}");
}

#[test]
fn port_trace_error_cardinality_cap_exceeded_display() {
    let err = PortTraceError::CardinalityCapExceeded { limit: 1000, current: 1001 };
    let s = err.to_string();
    assert!(s.contains("cardinality cap exceeded"), "got: {s}");
    assert!(s.contains("1000"), "expected limit in display; got: {s}");
    assert!(s.contains("1001"), "expected current in display; got: {s}");
}

#[test]
fn port_trace_error_backend_export_display() {
    let err = PortTraceError::BackendExport("connection refused to jaeger:14268".into());
    let s = err.to_string();
    assert!(s.contains("backend export error"), "got: {s}");
    assert!(s.contains("connection refused"), "got: {s}");
}

#[test]
fn port_trace_error_debug_output_is_non_empty() {
    for err in [
        PortTraceError::BufferPoisoned("a".into()),
        PortTraceError::FlushFailed("b".into()),
        PortTraceError::CardinalityCapExceeded { limit: 10, current: 11 },
        PortTraceError::BackendExport("c".into()),
    ] {
        let s = format!("{err:?}");
        assert!(!s.is_empty(), "Debug output must not be empty; variant: {s}");
    }
}

#[test]
fn port_trace_error_all_variants_are_std_error_leaf() {
    use std::error::Error;
    let variants: Vec<PortTraceError> = vec![
        PortTraceError::BufferPoisoned("x".into()),
        PortTraceError::FlushFailed("y".into()),
        PortTraceError::CardinalityCapExceeded { limit: 5, current: 6 },
        PortTraceError::BackendExport("z".into()),
    ];
    for err in &variants {
        assert!(
            err.source().is_none(),
            "All PortTraceError variants are leaf errors; source() must return None. Variant: {err:?}"
        );
    }
}
