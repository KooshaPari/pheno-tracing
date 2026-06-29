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

#![warn(missing_docs)]

pub mod adapters;
pub mod compat;
pub mod error;
pub mod port;
pub mod sampling;

pub use error::TraceError;
pub use port::{SpanId, SpanKind, TraceId, TraceOperation, TracePort, TraceResult};
pub use sampling::{
    AlwaysSampler, NeverSampler, ParentBasedSampler, RateLimitSampler, Sampler, SamplingDecision,
    SpanContext, TailBasedSampler,
};

// =============================================================================
// Hexagonal port aliases (v12-04 — sampling-policy port surface)
//
// Per ADR-014, the fleet uses `Port` trait + `Adapter` impl as the
// hexagonal port surface. The `sampling` module defines the canonical
// `Sampler` Port trait plus a `SpanContext` carrier type. The aliases below
// re-export those types under the spec-mandated names so consumers can
// write the fleet-port surface as:
//
//     use pheno_tracing::{HexSamplingPort, SamplingContext, SamplingDecision,
//                         AlwaysOnSampler, AlwaysOffSampler, ParentBasedSampler};
//
// and adapters can implement `HexSamplingPort` directly. The `Sampler` /
// `SpanContext` names remain stable for backwards compatibility — both
// spellings refer to the same trait / type, so existing consumers do not
// need to migrate.
// =============================================================================

/// Hexagonal Port alias for [`sampling::Sampler`] (v12-04).
///
/// `HexSamplingPort` is the spec-mandated name for the sampling-decision
/// Port trait; it is a 1:1 alias of [`Sampler`] so either spelling works.
pub use sampling::Sampler as HexSamplingPort;

/// Hexagonal carrier alias for [`sampling::SpanContext`] (v12-04).
pub use sampling::SpanContext as SamplingContext;

/// Adapter that always records every span (v12-04 spec name).
pub use sampling::AlwaysSampler as AlwaysOnSampler;

/// Adapter that always drops every span (v12-04 spec name).
pub use sampling::NeverSampler as AlwaysOffSampler;

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
