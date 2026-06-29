//! Typed error types for tracing port operations.
//!
//! Replaces bare `Result<(), String>` with a proper `TraceError` enum so
//! consumers can match on specific failure modes (flush error, lock poisoning)
//! rather than parsing string messages. Retrofitted from audit L14 finding.

use thiserror::Error;

/// Errors that can occur during trace port operations.
///
/// Every `TracePort` method returns either `Ok` or a `TraceError` variant
/// with enough context for structured logging, alerting, and programmatic
/// recovery — not just a human-readable string.
#[derive(Debug, Error)]
pub enum TraceError {
    /// The underlying trace backend failed to flush buffered spans.
    ///
    /// The inner string is a human-readable description provided by the
    /// backend (e.g. "OTLP export returned status 503").
    #[error("flush failed: {0}")]
    Flush(String),

    /// A synchronisation primitive was poisoned because a previous holder
    /// panicked while holding the lock.
    ///
    /// The adapter recovers the inner data and continues, but the error
    /// is surfaced so callers can decide whether to propagate or log it.
    #[error("lock poisoned: {0}")]
    LockPoisoned(String),
}
