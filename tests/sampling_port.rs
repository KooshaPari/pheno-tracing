//! Integration test for the v12-04 sampling-policy port surface.
//!
//! Verifies the spec-mandated `HexSamplingPort` trait and its 3 in-tree
//! adapters (`AlwaysOnSampler`, `AlwaysOffSampler`, `ParentBasedSampler`)
//! are reachable through the public API, and that the alias surface is
//! wire-compatible with the canonical [`pheno_tracing::Sampler`] names
//! (both spellings refer to the same trait / type — backwards compatible).
//!
//! ## Coverage
//!
//! - `HexSamplingPort` is `Send + Sync` and is the same trait as `Sampler`.
//! - `SamplingContext` is the same type as `SpanContext`.
//! - `AlwaysOnSampler` (alias for `AlwaysSampler`) records every span.
//! - `AlwaysOffSampler` (alias for `NeverSampler`) drops every span.
//! - `ParentBasedSampler` honors the parent's sampled bit (sampled parent
//!   → child records; unsampled parent → child drops).
//! - `dyn HexSamplingPort` is object-safe (compile-time check).
//! - The port can be driven by config: a `Sampler`-typed slot in a
//!   config-driven consumer is satisfied by any of the 3 adapters.
//!
//! ## On the "config-driven sampler trait" framing
//!
//! The spec says the port is "config-driven", meaning consumers wire the
//! active sampler at startup from configuration (env, TOML, etc.) and the
//! rest of the call graph depends only on the [`HexSamplingPort`] trait.
//! We verify the indirection is type-safe by passing each adapter to a
//! generic function that accepts `&dyn HexSamplingPort` — the same
//! pattern a config loader would use.

use pheno_tracing::{
    AlwaysOffSampler, AlwaysOnSampler, HexSamplingPort, ParentBasedSampler, Sampler,
    SamplingContext, SamplingDecision, SpanContext,
};

// =============================================================================
// Helpers — exercise the spec-mandated alias surface.
// =============================================================================

/// Generic function that consumes the active sampler (the "config-driven
/// consumer" pattern). Anything implementing `HexSamplingPort` is
/// acceptable; the test body calls this with each of the 3 in-tree
/// adapters and asserts the expected decision.
fn decide(sampler: &dyn HexSamplingPort, ctx: &SamplingContext) -> SamplingDecision {
    sampler.should_sample(ctx)
}

/// True when the spec-mandated alias type and the canonical type are the
/// same type (compile-time guarantee — no runtime overhead).
fn _assert_same_trait_object(_a: &dyn HexSamplingPort, _b: &dyn Sampler) {}

// =============================================================================
// Tests
// =============================================================================

/// Test 1 — `HexSamplingPort` is the same trait as `Sampler`.
///
/// Re-exporting a trait under a new name produces the same type
/// identifier; we pin that here so a future refactor cannot accidentally
/// split them into two traits (e.g. by adding an associated type to
/// `Sampler` without mirroring it on `HexSamplingPort`).
#[test]
fn hex_sampling_port_is_sampler_under_new_name() {
    #[allow(clippy::extra_unused_type_parameters)]
    fn _check_same<T: ?Sized>() {
        // Both traits must be the same `dyn`-compatible shape; the alias
        // is a textual rename, not a separate trait.
        const _: fn() = || {
            let f: fn(&dyn Sampler, &SpanContext) -> SamplingDecision = |s, c| s.should_sample(c);
            let _g: fn(&dyn HexSamplingPort, &SamplingContext) -> SamplingDecision = f;
        };
        // Reference both to silence "unused" warnings on the const block.
        let _: fn(&dyn Sampler, &SpanContext) -> SamplingDecision = |s, c| s.should_sample(c);
        let _: fn(&dyn HexSamplingPort, &SamplingContext) -> SamplingDecision =
            |s, c| s.should_sample(c);
    }
    _check_same::<dyn Sampler>();
    _check_same::<dyn HexSamplingPort>();
}

/// Test 2 — `SamplingContext` is the same type as `SpanContext`.
#[test]
fn sampling_context_is_span_context_under_new_name() {
    // Compile-time: constructing a `SamplingContext` from a `SpanContext`
    // value must be a no-op (same type).
    let span_ctx: SpanContext = SpanContext::root("trace-1", "span-1", true);
    let samp_ctx: SamplingContext = span_ctx.clone();
    assert_eq!(span_ctx, samp_ctx, "SamplingContext must equal SpanContext");
    assert!(samp_ctx.is_sampled(), "sampled bit must be set");
}

/// Test 3 — `AlwaysOnSampler` records every span.
///
/// The spec name is `AlwaysOnSampler`; under the hood it is the same type
/// as the canonical `AlwaysSampler`. We verify behavior end-to-end.
#[test]
fn always_on_sampler_records_every_span() {
    let sampler = AlwaysOnSampler;
    assert_eq!(sampler.name(), "always");
    assert_eq!(sampler.name(), pheno_tracing::AlwaysSampler.name());

    // Sampled context → record.
    let sampled = SamplingContext::root("t", "s", true);
    assert_eq!(decide(&sampler, &sampled), SamplingDecision::Record);

    // Unsampled context → still record (this is the difference vs
    // ParentBased: AlwaysOn is unconditional).
    let unsampled = SamplingContext::root("t", "s", false);
    assert_eq!(decide(&sampler, &unsampled), SamplingDecision::Record);

    // No-parent context with random flags → still record.
    let noisy =
        SamplingContext::root("t", "s", false).with_parent(SamplingContext::root("p", "p", false));
    assert_eq!(decide(&sampler, &noisy), SamplingDecision::Record);
}

/// Test 4 — `AlwaysOffSampler` drops every span.
///
/// The spec name is `AlwaysOffSampler`; under the hood it is the same
/// type as the canonical `NeverSampler`. We verify behavior end-to-end.
#[test]
fn always_off_sampler_drops_every_span() {
    let sampler = AlwaysOffSampler;
    assert_eq!(sampler.name(), "never");
    assert_eq!(sampler.name(), pheno_tracing::NeverSampler.name());

    // Sampled context → still drop (the point of AlwaysOff).
    let sampled = SamplingContext::root("t", "s", true);
    assert_eq!(decide(&sampler, &sampled), SamplingDecision::Drop);

    // Unsampled context → drop.
    let unsampled = SamplingContext::root("t", "s", false);
    assert_eq!(decide(&sampler, &unsampled), SamplingDecision::Drop);

    // Sampled ancestor → still drop (AlwaysOff ignores upstream intent).
    let child_of_sampled =
        SamplingContext::root("c", "c", false).with_parent(SamplingContext::root("p", "p", true));
    assert_eq!(
        decide(&sampler, &child_of_sampled),
        SamplingDecision::Drop,
        "AlwaysOff must drop unconditionally, even when an ancestor is sampled"
    );
}

/// Test 5 — `ParentBasedSampler` honors the parent's sampled bit.
#[test]
fn parent_based_sampler_honors_parent_decision() {
    let sampler = ParentBasedSampler::new();
    assert_eq!(sampler.name(), "parent-based");

    // Sampled parent → child records.
    let parent = SamplingContext::root("trace-x", "span-parent", true);
    let child = SamplingContext::root("trace-x", "span-child", false).with_parent(parent);
    assert_eq!(
        decide(&sampler, &child),
        SamplingDecision::Record,
        "sampled parent must propagate to the child"
    );

    // Unsampled parent → child drops. The child's own `sampled` flag
    // MUST be `false` here: the `ParentBasedSampler` consults the
    // child's flag first and only falls back to the parent when the
    // child has no opinion (matches the existing in-file test
    // `parent_based_honors_unsampled_parent`).
    let parent = SamplingContext::root("trace-y", "span-parent", false);
    let child = SamplingContext::root("trace-y", "span-child", false).with_parent(parent);
    assert_eq!(
        decide(&sampler, &child),
        SamplingDecision::Drop,
        "unsampled parent must propagate to the child (when child has no opinion)"
    );

    // Sanity: a child that *does* have an opinion (sampled=true) records
    // even with an unsampled parent — the child's flag wins.
    let parent = SamplingContext::root("trace-w", "span-parent", false);
    let child = SamplingContext::root("trace-w", "span-child", true).with_parent(parent);
    assert_eq!(
        decide(&sampler, &child),
        SamplingDecision::Record,
        "child's own sampled flag takes precedence over an unsampled parent"
    );

    // Root (no parent) — consult the context's own flag.
    let sampled_root = SamplingContext::root("trace-z", "span-root", true);
    let unsampled_root = SamplingContext::root("trace-z", "span-root", false);
    assert_eq!(decide(&sampler, &sampled_root), SamplingDecision::Record);
    assert_eq!(decide(&sampler, &unsampled_root), SamplingDecision::Drop);
}

/// Test 6 — `dyn HexSamplingPort` is object-safe.
///
/// Compile-time check: the trait can be used as `dyn HexSamplingPort`. If
/// a future refactor accidentally adds a generic method or a `Self: Sized`
/// bound to `HexSamplingPort` (or to `Sampler`, since they're the same
/// trait), this test fails to compile.
#[test]
fn hex_sampling_port_is_object_safe() {
    fn _accept_dyn(_s: &dyn HexSamplingPort) {}
    _accept_dyn(&AlwaysOnSampler);
    _accept_dyn(&AlwaysOffSampler);
    _accept_dyn(&ParentBasedSampler::new());

    // All three can be moved into a `Vec` of trait objects, the canonical
    // "config-driven consumer" wiring.
    let _samplers: Vec<Box<dyn HexSamplingPort>> = vec![
        Box::new(AlwaysOnSampler),
        Box::new(AlwaysOffSampler),
        Box::new(ParentBasedSampler::new()),
    ];
}

/// Test 7 — config-driven dispatch (the "port" use case).
///
/// Models a config-driven consumer: given a (textual) name, pick the
/// matching in-tree adapter and run the decision. This is the
/// indirection pattern the spec calls out — the rest of the call graph
/// depends only on `&dyn HexSamplingPort`, not on the concrete type.
#[test]
fn config_driven_dispatch_via_hex_sampling_port() {
    // Mimic a config loader: string → adapter. The "config" in this
    // test is just a list; the production code reads from env / TOML.
    let configs: &[(&str, Box<dyn HexSamplingPort>)] = &[
        ("always-on", Box::new(AlwaysOnSampler)),
        ("always-off", Box::new(AlwaysOffSampler)),
        ("parent-based", Box::new(ParentBasedSampler::new())),
    ];

    let sampled_parent = SamplingContext::root("trace", "parent", true);
    let unsampled_parent = SamplingContext::root("trace", "parent", false);

    for (name, sampler) in configs {
        let sampled_child =
            SamplingContext::root("trace", "child", false).with_parent(sampled_parent.clone());
        let unsampled_child =
            SamplingContext::root("trace", "child", false).with_parent(unsampled_parent.clone());

        match *name {
            "always-on" => {
                assert_eq!(
                    sampler.should_sample(&sampled_child),
                    SamplingDecision::Record
                );
                assert_eq!(
                    sampler.should_sample(&unsampled_child),
                    SamplingDecision::Record
                );
            }
            "always-off" => {
                assert_eq!(
                    sampler.should_sample(&sampled_child),
                    SamplingDecision::Drop
                );
                assert_eq!(
                    sampler.should_sample(&unsampled_child),
                    SamplingDecision::Drop
                );
            }
            "parent-based" => {
                assert_eq!(
                    sampler.should_sample(&sampled_child),
                    SamplingDecision::Record,
                    "parent-based with sampled parent must record the child"
                );
                assert_eq!(
                    sampler.should_sample(&unsampled_child),
                    SamplingDecision::Drop,
                    "parent-based with unsampled parent must drop the child"
                );
            }
            _ => panic!("unknown sampler name: {name}"),
        }
    }
}

/// Test 8 — backwards compatibility.
///
/// The `Sampler` / `AlwaysSampler` / `NeverSampler` / `SpanContext` names
/// must still resolve to the same types after the v12-04 alias work. A
/// consumer that imports the old names must not break.
#[test]
fn legacy_names_still_resolve() {
    // The legacy types are still reachable at the crate root.
    let _sampler: &dyn Sampler = &ParentBasedSampler::new();
    let _ctx: SpanContext = SpanContext::root("t", "s", true);
    let _on: pheno_tracing::AlwaysSampler = pheno_tracing::AlwaysSampler;
    let _off: pheno_tracing::NeverSampler = pheno_tracing::NeverSampler;

    // The legacy and new names refer to the same types.
    let _same_trait: fn(&dyn Sampler, &SpanContext) -> SamplingDecision = |s, c| s.should_sample(c);
    let _same_trait_via_alias: fn(&dyn HexSamplingPort, &SamplingContext) -> SamplingDecision =
        |s, c| s.should_sample(c);

    // A legacy-typed function pointer can be coerced to the alias-typed
    // function pointer (one is a rename of the other; same signature).
    let legacy_fn: fn(&dyn Sampler, &SpanContext) -> SamplingDecision = |s, c| s.should_sample(c);
    let aliased_fn: fn(&dyn HexSamplingPort, &SamplingContext) -> SamplingDecision = legacy_fn;
    // Smoke call to silence "unused" — the decision is meaningless, we
    // only care that the type coercion compiled.
    let _ = aliased_fn(&AlwaysOnSampler, &SpanContext::root("t", "s", false));
}
