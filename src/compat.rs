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

//==============================================================================
// Tests
//==============================================================================
//
// Unit tests covering the 0.1→0.2 forward-compat shim. The integration test
// suite at `tests/tracing-0-2-compat.rs` exercises the `tracing-0-2` Cargo
// feature path; these inline tests cover the always-on default (V0_1 today)
// so the coverage gate (ADR-023 Rule 3.1: 80 % lib) stays green regardless
// of which feature flag CI activates.

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // TracingVersion
    // -------------------------------------------------------------------------

    #[test]
    fn tracing_version_current_is_v0_1() {
        // Today (pre-0.2 GA), the crate depends on `tracing = "0.1"`, so the
        // shim's compile-time version is unconditionally V0_1. The moment 0.2
        // GA lands, this constant flips — the assertion below is the
        // forward-compat sentinel for downstream code that branches on it.
        assert_eq!(TracingVersion::current(), TracingVersion::V0_1);
    }

    #[test]
    fn tracing_version_default_matches_current() {
        // `Default::default()` MUST return the same value as `current()`. This
        // is the contract for downstream code that writes
        // `TracingVersion::default()` in struct field initializers without
        // importing `current`.
        assert_eq!(TracingVersion::default(), TracingVersion::current());
    }

    #[test]
    fn tracing_version_display_v0_1() {
        // The Display impl is the wire-level contract for downstream code
        // that serializes the version (e.g. into OTLP resource attributes).
        assert_eq!(TracingVersion::V0_1.to_string(), "0.1");
    }

    #[test]
    fn tracing_version_display_v0_2() {
        // Even though the crate never actually returns V0_2 today, the
        // Display impl must produce the right string so future code can
        // format the value without first checking which variant it is.
        assert_eq!(TracingVersion::V0_2.to_string(), "0.2");
    }

    #[test]
    fn tracing_version_const_matches_current() {
        // `TRACING_VERSION` is a `const`; this test catches a future PR that
        // updates `current()` but forgets the static initializer (or vice
        // versa).
        assert_eq!(TRACING_VERSION, TracingVersion::current());
    }

    // -------------------------------------------------------------------------
    // SubscriberKind
    // -------------------------------------------------------------------------

    #[test]
    fn subscriber_kind_current_matches_version() {
        // `SubscriberKind::current()` is defined as a function of
        // `TRACING_VERSION`. Today that's V0_1 → Subscriber. When 0.2 lands,
        // the same logic maps V0_2 → Collector.
        assert_eq!(
            SubscriberKind::current(),
            match TRACING_VERSION {
                TracingVersion::V0_1 => SubscriberKind::Subscriber,
                TracingVersion::V0_2 => SubscriberKind::Collector,
            }
        );
        // Concrete today: Subscriber (the 0.1 backend name).
        assert_eq!(SubscriberKind::current(), SubscriberKind::Subscriber);
    }

    #[test]
    fn subscriber_kind_default_matches_current() {
        assert_eq!(SubscriberKind::default(), SubscriberKind::current());
    }

    #[test]
    fn current_backend_kind_const_matches_subscriber_kind_current() {
        // `current_backend_kind()` is the free-function re-export of
        // `SubscriberKind::current()` for callers who want a one-liner import.
        assert_eq!(current_backend_kind(), SubscriberKind::current());
    }

    // -------------------------------------------------------------------------
    // TracingBackend facade
    // -------------------------------------------------------------------------

    #[test]
    fn tracing_backend_default_uses_current_backend_kind() {
        // `TracingBackend::default()` is `Self { kind: current_backend_kind() }`
        // — i.e. the default tracks whatever `tracing` version the crate is
        // built against. The test guards against a future refactor that
        // hardcodes `Subscriber` in `Default`.
        let backend = TracingBackend::default();
        assert_eq!(backend.kind(), SubscriberKind::current());
        assert_eq!(backend.kind(), current_backend_kind());
    }

    #[test]
    fn tracing_backend_new_matches_default() {
        // `new()` is the documented constructor; it must equal `default()`
        // because both route through `current_backend_kind()`.
        let a = TracingBackend::new();
        let b = TracingBackend::default();
        assert_eq!(a.kind(), b.kind());
        assert_eq!(a.version(), b.version());
    }

    #[test]
    fn tracing_backend_with_kind_subscriber() {
        // The `with_kind` constructor is the test-and-mock hook: it lets
        // consumers simulate the "other" backend (Collector on 0.1, or vice
        // versa) without flipping the Cargo feature.
        let backend = TracingBackend::with_kind(SubscriberKind::Subscriber);
        assert_eq!(backend.kind(), SubscriberKind::Subscriber);
        assert_eq!(backend.version(), TracingVersion::V0_1);
    }

    #[test]
    fn tracing_backend_with_kind_collector() {
        // Same as above but for the 0.2 path. This is the value that
        // downstream consumers will inspect once 0.2 GA lands.
        let backend = TracingBackend::with_kind(SubscriberKind::Collector);
        assert_eq!(backend.kind(), SubscriberKind::Collector);
        assert_eq!(backend.version(), TracingVersion::V0_2);
    }

    #[test]
    fn tracing_backend_version_round_trip() {
        // The kind → version mapping must be total and lossless so that
        // downstream code can do `backend.version()` and rely on the result.
        for kind in [SubscriberKind::Subscriber, SubscriberKind::Collector] {
            let backend = TracingBackend::with_kind(kind);
            match kind {
                SubscriberKind::Subscriber => {
                    assert_eq!(backend.version(), TracingVersion::V0_1)
                }
                SubscriberKind::Collector => {
                    assert_eq!(backend.version(), TracingVersion::V0_2)
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // SubscriberAdapter + CollectorAdapter trait surface
    // -------------------------------------------------------------------------

    /// A minimal `SubscriberAdapter` impl used to verify the trait surface
    /// compiles and the blanket `CollectorAdapter for T: SubscriberAdapter`
    /// impl applies. Holds no state — this is a compile-time check more
    /// than a runtime test.
    struct NoopAdapter;

    impl SubscriberAdapter for NoopAdapter {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, span: &tracing::Span) -> tracing::Span {
            span.clone()
        }
        fn record(&self, _span: &tracing::Span, _values: &tracing::span::Record<'_>) {}
        fn record_follows_from(
            &self,
            _span: &tracing::Span,
            _follows: &tracing::span::Record<'_>,
        ) {
        }
        fn event(&self, _event: &tracing::Event<'_>) {}
        fn enter(&self, _span: &tracing::Span) {}
        fn exit(&self, _span: &tracing::Span) {}
    }

    #[test]
    fn subscriber_adapter_trait_is_implemented_by_noop_adapter() {
        // Smoke test: a `NoopAdapter` value exists, satisfies the trait
        // surface, and can be passed to a generic function bounded on
        // `SubscriberAdapter`. The point is compile-time confirmation that
        // the trait shape is reachable; we don't invoke methods with
        // constructed `Metadata` / `Event` / `Record` values because those
        // upstream types are hard to build from outside `tracing`'s crate
        // (their `Metadata` is `Option`-wrapped internally and the
        // constructors are not always `pub`).
        let adapter = NoopAdapter;
        fn requires_subscriber<T: SubscriberAdapter>(_: &T) {}
        requires_subscriber(&adapter);
    }

    #[test]
    fn subscriber_adapter_max_level_default_is_trace() {
        // The trait provides `max_level_hint` with a default of `Level::TRACE`
        // (the most permissive). Downstream adapters that want a stricter
        // cap override it; the default must remain unchanged.
        let adapter = NoopAdapter;
        assert_eq!(adapter.max_level_hint(), tracing::Level::TRACE);
    }

    #[test]
    fn blanket_collector_adapter_impl_applies() {
        // On tracing 0.1 (today), `CollectorAdapter` is a blanket supertrait
        // of `SubscriberAdapter`: every `SubscriberAdapter` impl is
        // automatically a `CollectorAdapter` impl. This test asserts that
        // a function generic over `CollectorAdapter` accepts our `NoopAdapter`
        // — proving the blanket impl is reachable from outside the module.
        fn requires_collector<T: CollectorAdapter>(_: &T) {}
        let adapter = NoopAdapter;
        requires_collector(&adapter);
    }

    // -------------------------------------------------------------------------
    // Macro re-exports
    // -------------------------------------------------------------------------

    #[test]
    fn macro_reexports_resolve() {
        // The `pub use tracing::{debug, error, info, instrument, span, trace, warn};`
        // line is the central import-path contract for downstream consumers.
        // If a future refactor accidentally drops one of these re-exports
        // (or mistypes the upstream name), this test catches it at compile
        // time. We just need each name to resolve to a callable macro — we
        // don't care about the resulting log lines (no subscriber is
        // installed; the macros short-circuit silently).
        info!("re-export smoke test");
        warn!("re-export smoke test");
        error!("re-export smoke test");
        debug!("re-export smoke test");
        trace!("re-export smoke test");
        // `span!` requires a level when called with one positional arg, so
        // exercise it as `span!(level, name)` to confirm the re-export
        // resolves to the same upstream macro signature downstream
        // consumers rely on.
        let _g = span!(Level::INFO, "re-export smoke test");
        // `instrument` is an attribute macro; just check it's importable
        // by writing a function decorated with it. The function is `pub`
        // only because `instrument` requires the item to be at least as
        // visible as the function it decorates; the test crate's root is
        // its only consumer.
        #[instrument]
        fn decorated_fn() {}
        let _ = decorated_fn;
    }

    #[test]
    fn level_reexport_matches_tracing() {
        // `pub use tracing::Level` — downstream code that writes
        // `pheno_tracing::compat::Level::INFO` must get the exact same enum
        // as `tracing::Level::INFO`. Identity check.
        assert_eq!(Level::INFO, tracing::Level::INFO);
        assert_eq!(Level::WARN, tracing::Level::WARN);
        assert_eq!(Level::ERROR, tracing::Level::ERROR);
        assert_eq!(Level::DEBUG, tracing::Level::DEBUG);
        assert_eq!(Level::TRACE, tracing::Level::TRACE);
    }

    // -------------------------------------------------------------------------
    // Shim metadata
    // -------------------------------------------------------------------------

    #[test]
    fn shim_version_matches_cargo_pkg_version() {
        // `SHIM_VERSION` is `env!("CARGO_PKG_VERSION")`, so it MUST equal
        // the package version reported by Cargo. If a future PR moves the
        // constant to a different source (e.g. a hand-maintained string),
        // this test catches the drift.
        assert_eq!(SHIM_VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn shim_version_is_semver_pre_release() {
        // The shim is pre-release per the module docs (this version is
        // `0.3.0-pre.0`). The test guards against an accidental GA bump that
        // forgets to clear the `-pre.*` suffix — which would be a
        // semver-violating stable release of a still-experimental surface.
        let v = SHIM_VERSION;
        assert!(
            v.contains("-pre.") || v.contains("-alpha") || v.contains("-beta"),
            "pheno-tracing compat shim must keep a pre-release tag; got: {v}"
        );
    }

    // -------------------------------------------------------------------------
    // Hash / Eq contracts (used by downstream code that keys on these)
    // -------------------------------------------------------------------------

    #[test]
    fn tracing_version_is_hashable_and_eq() {
        // Downstream code uses `TracingVersion` as a HashMap key (e.g. for
        // per-version feature flags). Verify the derives are stable.
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TracingVersion::V0_1);
        set.insert(TracingVersion::V0_2);
        set.insert(TracingVersion::V0_1); // duplicate, no-op
        assert_eq!(set.len(), 2);
        assert!(set.contains(&TracingVersion::V0_1));
        assert!(set.contains(&TracingVersion::V0_2));
    }

    #[test]
    fn subscriber_kind_is_hashable_and_eq() {
        // Same as above for `SubscriberKind`.
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(SubscriberKind::Subscriber);
        set.insert(SubscriberKind::Collector);
        set.insert(SubscriberKind::Subscriber); // duplicate
        assert_eq!(set.len(), 2);
    }
}
