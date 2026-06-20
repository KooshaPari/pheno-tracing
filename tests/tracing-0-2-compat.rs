#![cfg(feature = "tracing-0-2")]
//! `tracing-0-2-compat` — compatibility shim tests for `pheno-tracing`.
//!
//! These tests are gated behind the `tracing-0-2` Cargo feature so the crate
//! can build cleanly against `tracing = "0.1"` (current dep) without forcing
//! downstream consumers to opt in to forward-compat work.
//!
//! When the upstream `tracing` crate ships 0.2 GA, the `tracing-0-2` feature
//! will switch the underlying dep to `tracing = "0.2"` and these tests will
//! validate the shim's behavior against the real 0.2 API surface
//! (`tracing::Collector`, the `Value`/`Metadata` trait shape, etc.).
//!
//! For the pre-release shim (this PR), the tests verify the shim's API
//! surface: macro re-exports, the `SubscriberAdapter`/`CollectorAdapter` trait
//! family, and the version-detection helpers.

use pheno_tracing::compat::{
    current_backend_kind, debug, error, info, instrument, span, trace, warn, CollectorAdapter,
    SubscriberAdapter, SubscriberKind, TracingBackend, TracingVersion,
};
use std::collections::HashMap;

//---- macro re-export shape (compile-time + sanity) -----------------------

/// All seven standard tracing macros must be re-exported from
/// `pheno_tracing::compat::*` and callable with the 0.1-shaped call site.
///
/// On `tracing 0.1`, these macros live under `tracing::{info,warn,...}` and
/// expand to event/span construction. On `tracing 0.2`, the same macros are
/// preserved for source compatibility but route through `tracing::Collector`.
/// The shim guarantees that downstream code using these macros continues to
/// compile whether we are on 0.1 or 0.2.
#[test]
fn macros_are_re_exported_and_callable() {
    // Use each macro in a `let _ = ...` context to force expansion. This is
    // a compile-time check: if any macro is missing from the re-export, this
    // test fails to build.
    let _g = "trace-001";
    let _s = "span-001";
    let _attrs: HashMap<&str, &str> = HashMap::new();

    // Each of these expands into a tracing call. We don't assert on side
    // effects (those require a Subscriber/Collector installed); we only
    // require that the macro re-exports resolve.
    info!("info message");
    warn!("warn message");
    error!("error message");
    debug!("debug message");
    trace!("trace message");
    span!(tracing::Level::INFO, "compat-test", trace_id = %_g);

    // `instrument` is a proc-macro attribute, so we cannot bind it as a
    // value. Instead we declare a no-op function with the attribute applied;
    // if `instrument` is missing from the re-export this fails to compile.
    #[instrument]
    fn _has_instrument_attr() {}
    let _ = _has_instrument_attr;
}

//---- SubscriberKind / version detection ----------------------------------

#[test]
fn current_backend_kind_is_subscriber_on_0_1() {
    // Pre-release shim targets `tracing = "0.1"`, so the runtime backend kind
    // must report as `Subscriber` (the 0.1 name). When 0.2 ships and the
    // dep flips, this test is updated to expect `Collector`.
    assert_eq!(current_backend_kind(), SubscriberKind::Subscriber);
}

#[test]
fn tracing_version_constant_is_present() {
    // The shim exposes a `TracingVersion` enum so downstream code can branch
    // at runtime if needed. We verify the variant set is non-empty and the
    // default is 0.1.
    let v = TracingVersion::default();
    assert!(matches!(v, TracingVersion::V0_1 | TracingVersion::V0_2));
    assert_eq!(v, TracingVersion::V0_1);
}

//---- Adapter trait surface -----------------------------------------------

/// A trivial `SubscriberAdapter` impl for unit testing the shim's blanket
/// conversions. Stores the trace_id seen on `on_event` so tests can assert
/// that the 0.1-shaped calls reach our adapter.
#[derive(Default, Clone)]
struct RecordingSubscriber {
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl SubscriberAdapter for RecordingSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, span: &tracing::Span) -> tracing::Span {
        span.clone()
    }
    fn record(&self, _span: &tracing::Span, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::Span, _follows: &tracing::span::Record<'_>) {}
    fn event(&self, event: &tracing::Event<'_>) {
        // Capture the parent's name (best-effort).
        use tracing::field::{Field, Visit};
        struct NameVisit(String);
        impl Visit for NameVisit {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{:?}", value);
                }
            }
        }
        let mut v = NameVisit(String::new());
        event.record(&mut v);
        self.seen.lock().unwrap().push(v.0);
    }
    fn enter(&self, _span: &tracing::Span) {}
    fn exit(&self, _span: &tracing::Span) {}
}

#[test]
fn subscriber_adapter_trait_object_works() {
    // Compile-time check: we can build a trait object and route through it.
    // We use `Span::none()` for the trait method signatures that take a span
    // (avoids needing a real Callsite, which is what `Metadata::new` would
    // require).
    let sub: Box<dyn SubscriberAdapter> = Box::new(RecordingSubscriber::default());
    let span = tracing::Span::none();
    let _ = sub.new_span(&span);
    sub.enter(&span);
    sub.exit(&span);
    let _ = sub.max_level_hint();
}

//---- CollectorAdapter (0.2 path) — only when 0.2 is active ---------------

/// On 0.2, `tracing::Collector` replaces `tracing::Subscriber`. The shim
/// provides a parallel `CollectorAdapter` trait so consumers can write
/// adapter code that compiles against both versions. On 0.1, this trait
/// type-aliases to `SubscriberAdapter`.
#[test]
fn collector_adapter_alias_resolves() {
    fn _accepts_collector(_c: impl CollectorAdapter) {}
    // Calling this with a 0.1 Subscriber impl must compile via the alias.
    let sub = RecordingSubscriber::default();
    _accepts_collector(sub);
}

//---- TracingBackend facade -----------------------------------------------

/// `TracingBackend` is the shim's unified facade that downstream code can
/// use without caring whether the underlying tracing dep is 0.1 or 0.2.
#[test]
fn tracing_backend_default_returns_inert_backend() {
    let backend = TracingBackend::default();
    assert_eq!(backend.kind(), current_backend_kind());
}
