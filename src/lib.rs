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
pub mod compat;
pub mod port;

pub use port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceResult};

// Re-export the `compat` module's macro family at the crate root for ergonomic
// imports. Downstream consumers can either write
//   use pheno_tracing::{info, span, instrument};
// or
//   use pheno_tracing::compat::{info, span, instrument};
// Both resolve to the same upstream `tracing` macros. The re-export at the
// crate root is the documented stable path; the `compat` module also exposes
// them so adapters that need the version-detection helpers can keep imports
// in one place.
pub use compat::{
    current_backend_kind, CollectorAdapter, SubscriberAdapter, SubscriberKind, TracingBackend,
    TracingVersion,
};
pub use compat::{debug, error, info, instrument, span, trace, warn};
