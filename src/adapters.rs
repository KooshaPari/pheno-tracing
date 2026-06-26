//! Adapter implementations for tracing backends.
//!
//! Two adapters ship with the crate:
//!
//! - [`InMemoryAdapter`] — for unit/integration tests; stores spans in an
//!   `Arc<Mutex<Vec<TraceOperation>>>` so tests can assert on what was
//!   submitted.
//! - [`StdoutAdapter`] — for local debugging; prints spans to stdout.

use crate::metrics;
use crate::port::{TraceOperation, TracePort, TraceResult, TraceStatus};
use std::sync::{Arc, Mutex};

/// In-memory adapter for testing.
///
/// Stores submitted spans in a thread-safe buffer so tests can assert on
/// what was submitted. Use `Default` or `new()` to construct.
#[derive(Default, Clone)]
pub struct InMemoryAdapter {
    /// Submitted spans retained for test assertions.
    pub spans: Arc<Mutex<Vec<TraceOperation>>>,
}

impl InMemoryAdapter {
    /// Construct a fresh, empty in-memory adapter.
    pub fn new() -> Self {
        Self {
            spans: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl TracePort for InMemoryAdapter {
    async fn submit(&self, op: TraceOperation) -> TraceResult {
        let mut spans = match self.spans.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                // L62 (error rate) observability adoption (v14 cycle-4 T7).
                // The lock was poisoned by a panicking holder; we recover the
                // data rather than crashing the trace path.
                metrics::record_error("pheno_tracing.in_memory.submit", "lock_poisoned");
                poisoned.into_inner()
            }
        };
        spans.push(op.clone());
        TraceResult {
            trace_id: op.trace_id,
            span_id: op.span_id,
            status: TraceStatus::Ok,
        }
    }

    async fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Stdout adapter for local debugging.
///
/// Prints each span to stdout in the form `[TRACE] trace=<id> span=<name>
/// kind=<kind>`. Useful for `cargo run` and one-off debugging; not for
/// production.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdoutAdapter;

#[async_trait::async_trait]
impl TracePort for StdoutAdapter {
    async fn submit(&self, op: TraceOperation) -> TraceResult {
        println!(
            "[TRACE] trace={} span={} kind={:?}",
            op.trace_id.0, op.name, op.kind
        );
        TraceResult {
            trace_id: op.trace_id,
            span_id: op.span_id,
            status: TraceStatus::Ok,
        }
    }

    async fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}
