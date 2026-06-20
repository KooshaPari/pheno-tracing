//! Forward-compatibility shim for `tracing` 0.1 → 0.2.
//!
//! ## Why
//!
//! `pheno-tracing` is the canonical substrate for distributed tracing across
//! the 14 Rust repos in the pheno-* fleet that depend on `tracing = "0.1"`.
//! When `tracing` 0.2 ships, the upstream API is expected to rename
//! `tracing::Subscriber` → `tracing::Collector` and refine the trait shape for
//! `Value`/`Metadata`/`Event` (per the SOTA-async-trait-migration §3 tracing
//! research, this turn).
//!
//! To keep downstream consumers unblocked, this module exposes:
//!
//! 1. **Macro re-exports** — `info!`, `warn!`, `error!`, `debug!`, `trace!`,
//!    `span!`, `instrument` — pulled from the underlying `tracing` dep. On
//!    0.1 they expand as today; on 0.2 the same call sites resolve to the
//!    0.2 implementations without any source change in consumers.
//! 2. **`SubscriberAdapter` / `CollectorAdapter` traits** — thin wrappers
//!    over the 0.1 Subscriber trait shape (and the projected 0.2 Collector
//!    shape). On 0.1, `CollectorAdapter: SubscriberAdapter` (blanket impl)
//!    so existing Subscriber impls are also Collector impls. On 0.2 the
//!    shim flips the supertrait so that `CollectorAdapter` becomes the
//!    primary type.
//! 3. **`TracingBackend` / `TracingVersion` / `SubscriberKind`** — runtime
//!    facade for downstream code that needs to branch on which tracing
//!    version is active (rare; most consumers will not need this).
//!
//! ## Activation
//!
//! The shim is **always compiled** (no feature gate on `pheno-tracing` itself).
//! The opt-in `tracing-0-2` Cargo feature gates
//! `tests/tracing-0-2-compat.rs` only — so downstream consumers can enable
//! the feature in CI to exercise the forward-compat path once 0.2 lands,
//! without forcing every consumer to pull pre-release deps.
//!
//! ## Pre-release status
//!
//! This shim is **pre-release prep**. The actual `tracing` dep flip to 0.2
//! is a separate P2 task; see the SOTA research §3 tracing section for the
//! full migration plan.

use std::fmt;

//==============================================================================
// Version detection
//==============================================================================

/// Concrete `tracing` major version this crate is compiled against.
///
/// Today this is always `V0_1`. When the upstream `tracing` 0.2 GA lands, the
/// constant is bumped; downstream code that needs runtime branching can
/// match on this enum without depending on `tracing`'s own version APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TracingVersion {
    /// `tracing = "0.1"`. Subscriber-based API.
    V0_1,
    /// `tracing = "0.2"`. Collector-based API.
    V0_2,
}

impl Default for TracingVersion {
    fn default() -> Self {
        Self::current()
    }
}

impl TracingVersion {
    /// Returns the `tracing` version this crate was compiled against.
    ///
    /// Detected at compile time via the `tracing-0-2` Cargo feature: when
    /// that feature is enabled, the build is targeting 0.2; otherwise it is
    /// targeting 0.1. This is a forward declaration; today the feature is a
    /// no-op (the dep is 0.1 regardless), but the surface is locked so
    /// downstream code can rely on it.
    pub const fn current() -> Self {
        // Pre-release: the `tracing-0-2` feature is a forward-compat signal
        // only; today we always return V0_1 because the crate depends on
        // `tracing = "0.1"`. When 0.2 GA ships, the dep flips and this const
        // is updated to read the feature flag.
        TracingVersion::V0_1
    }
}

impl fmt::Display for TracingVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TracingVersion::V0_1 => f.write_str("0.1"),
            TracingVersion::V0_2 => f.write_str("0.2"),
        }
    }
}

/// Const tag for the `tracing` version this crate was compiled against.
pub const TRACING_VERSION: TracingVersion = TracingVersion::current();

//==============================================================================
// Subscriber vs. Collector — runtime tag
//==============================================================================

/// Which backend kind the active `tracing` version uses.
///
/// `Subscriber` is the 0.1 name; `Collector` is the projected 0.2 name.
/// Downstream code that wants to format log lines differently depending on
/// the backend (e.g. JSON for OTLP collectors, plain for stdout) can branch
/// on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubscriberKind {
    /// `tracing::Subscriber` (0.1).
    Subscriber,
    /// `tracing::Collector` (0.2).
    Collector,
}

impl Default for SubscriberKind {
    fn default() -> Self {
        Self::current()
    }
}

impl SubscriberKind {
    /// Returns the backend kind matching the compiled `tracing` version.
    pub const fn current() -> Self {
        match TRACING_VERSION {
            TracingVersion::V0_1 => SubscriberKind::Subscriber,
            TracingVersion::V0_2 => SubscriberKind::Collector,
        }
    }
}

/// Returns the active backend kind. Convenience wrapper around
/// [`SubscriberKind::current`].
pub const fn current_backend_kind() -> SubscriberKind {
    SubscriberKind::current()
}

//==============================================================================
// SubscriberAdapter — abstracts over tracing::Subscriber
//==============================================================================

/// Trait that mirrors `tracing::Subscriber` for the parts of the API most
/// commonly used by adapter authors in the pheno-* fleet.
///
/// On `tracing 0.1` this is the canonical trait. On `tracing 0.2` the same
/// method set is expected to be preserved under `tracing::Collector`; the
/// shim's `CollectorAdapter` trait (below) keeps a parallel surface so that
/// adapter code compiles unchanged against either version.
///
/// We intentionally do **not** re-export `tracing::Subscriber` here:
/// downstream consumers of the shim should depend on `SubscriberAdapter`
/// (and on `tracing::Subscriber` only when they need the full trait).
pub trait SubscriberAdapter: Send + Sync {
    /// See [`tracing::Subscriber::enabled`].
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool;

    /// See [`tracing::Subscriber::new_span`].
    fn new_span(&self, span: &tracing::Span) -> tracing::Span;

    /// See [`tracing::Subscriber::record`].
    fn record(&self, span: &tracing::Span, values: &tracing::span::Record<'_>);

    /// See [`tracing::Subscriber::record_follows_from`].
    fn record_follows_from(&self, span: &tracing::Span, follows: &tracing::span::Record<'_>);

    /// See [`tracing::Subscriber::event`].
    fn event(&self, event: &tracing::Event<'_>);

    /// See [`tracing::Subscriber::enter`].
    fn enter(&self, span: &tracing::Span);

    /// See [`tracing::Subscriber::exit`].
    fn exit(&self, span: &tracing::Span);

    /// Maximum verbosity the adapter is willing to record. Defaults to
    /// `Level::TRACE` (the most permissive). Override to constrain.
    fn max_level_hint(&self) -> tracing::Level {
        tracing::Level::TRACE
    }
}

//==============================================================================
// CollectorAdapter — Collector path (0.2)
//==============================================================================

/// `CollectorAdapter` is the shim's projected 0.2 counterpart to
/// `SubscriberAdapter`.
///
/// On `tracing 0.1` (today), `CollectorAdapter` is a blanket supertrait of
/// `SubscriberAdapter`: every existing Subscriber impl is automatically a
/// Collector impl, so adapter code that targets the "either version" shim
/// compiles unchanged.
///
/// On `tracing 0.2` (when the upstream dep flips), `CollectorAdapter` becomes
/// the primary trait and `SubscriberAdapter` is provided as a deprecated
/// alias. The exact split is decided when 0.2 GA is integrated; this
/// declaration is the placeholder.
pub trait CollectorAdapter: SubscriberAdapter {}

// Blanket impl: on 0.1, any SubscriberAdapter is a CollectorAdapter.
// When 0.2 lands, this blanket impl is removed and replaced with an
// explicit `impl CollectorAdapter for tracing::Collector` (or similar).
impl<T> CollectorAdapter for T where T: SubscriberAdapter {}

//==============================================================================
// TracingBackend — unified facade
//==============================================================================

/// Unified facade for the active tracing backend. Lets downstream code
/// route through a single handle regardless of whether the underlying
/// `tracing` dep is 0.1 or 0.2.
///
/// Today this is an inert tag carrying the backend kind. When the shim
/// matures (post-0.2 GA), this type will hold an `Arc<dyn SubscriberAdapter>`
/// (or `Arc<dyn CollectorAdapter>`) and route event/span calls into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TracingBackend {
    kind: SubscriberKind,
}

impl Default for TracingBackend {
    fn default() -> Self {
        Self {
            kind: current_backend_kind(),
        }
    }
}

impl TracingBackend {
    /// Construct a backend facade tagged with the currently-compiled kind.
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a backend facade tagged with an explicit kind. Useful for
    /// tests that want to verify the "other" branch.
    pub fn with_kind(kind: SubscriberKind) -> Self {
        Self { kind }
    }

    /// Returns the kind tag for this backend.
    pub fn kind(&self) -> SubscriberKind {
        self.kind
    }

    /// Returns the `tracing` version this backend corresponds to.
    pub fn version(&self) -> TracingVersion {
        match self.kind {
            SubscriberKind::Subscriber => TracingVersion::V0_1,
            SubscriberKind::Collector => TracingVersion::V0_2,
        }
    }
}

//==============================================================================
// Macro re-exports
//==============================================================================

// These re-exports give downstream consumers a stable import path:
// `use pheno_tracing::compat::{info, span, instrument, ...};`
//
// On `tracing 0.1` they expand to the upstream `tracing::{info, span, ...}`
// macros. On `tracing 0.2` (when the upstream `Subscriber` → `Collector`
// rename happens), the same call sites continue to compile because the
// macros themselves are stable across the version bump — only the
// underlying trait they call changes, and that's abstracted by
// `SubscriberAdapter`/`CollectorAdapter`.
pub use tracing::{debug, error, info, instrument, span, trace, warn};

//==============================================================================
// Blanket re-export of tracing's Level for convenience
//==============================================================================

/// Re-export of `tracing::Level` so downstream code can write
/// `pheno_tracing::compat::Level::INFO` without depending on `tracing`
/// directly. Keeps the forward-compat boundary in one place.
pub use tracing::Level;

//==============================================================================
// Internal marker for doc links (kept private; documents the shim's
// relationship to the upstream crate without exposing internal state).
//==============================================================================

#[doc(hidden)]
pub const SHIM_VERSION: &str = env!("CARGO_PKG_VERSION");
