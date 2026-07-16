//! Pheno Tracing — A port-driven distributed tracing substrate (ADR-036).
//!
//! This crate provides a clean port/adapter boundary for tracing operations,
//! so consumers in the pheno-* fleet can submit spans through a stable
//! `TracePort` trait while the underlying backend (in-memory, stdout, OTLP,
//! Jaeger, etc.) is swappable behind the adapter.
//!
//! # Quickstart
//!
//! ```rust
//! use pheno_tracing::adapters::InMemoryAdapter;
//! use pheno_tracing::port::{TraceId, SpanId, TraceOperation, SpanKind, TracePort};
//! use std::collections::HashMap;
//!
//! # async fn run() {
//! let adapter = InMemoryAdapter::new();
//! let op = TraceOperation {
//!     trace_id: TraceId("trace-001".into()),
//!     span_id: SpanId("span-001".into()),
//!     parent_span_id: None,
//!     kind: SpanKind::Internal,
//!     name: "test-span".into(),
//!     attributes: HashMap::new(),
//! };
//! let result = adapter.submit(op).await;
//! assert!(result.trace_id.0 == "trace-001");
//! # }
//! ```

pub mod adapters;
pub mod config;
pub mod env;
pub mod port;

/// Initialize the global tracing subscriber with structured JSON output.
///
/// This is the canonical entry point for all Phenotype services that want
/// structured logging. It configures `tracing-subscriber` with:
///
/// - JSON formatting to stderr
/// - `RUST_LOG` env-filter (defaults to `info`)
/// - Idempotent — safe to call multiple times (second+ calls are no-ops)
///
/// # Panics
///
/// Panics if the subscriber could not be installed (e.g. if another subscriber
/// was already registered by a different mechanism).
pub fn init() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = fmt::layer()
        .json()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_current_span(true)
        .with_line_number(true)
        .with_file(true)
        .with_filter(env_filter);

    // `try_init` makes this idempotent — second call is silently ignored.
    let _ = tracing_subscriber::registry().with(fmt_layer).try_init();
}

pub use config::{Format, TracingConfig};
pub use env::OtlpEnvConfig;
pub use port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceResult};
