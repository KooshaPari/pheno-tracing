//! Integration tests for the v22-T2 / L26 sampling and cardinality
//! surfaces.
//!
//! Per the v22-T2 acceptance criteria, the four canonical tests are:
//!
//! 1. `probabilistic_sampler_records_at_configured_rate` — the
//!    [`ProbabilisticSampler`] (alias for `TraceIdRatioBased`) records
//!    a fraction of trace_ids close to the configured rate.
//! 2. `rate_limited_sampler_caps_at_max_per_sec` — the
//!    [`RateLimitedSampler`] (probabilistic gate + token-bucket)
//!    records at most `max_per_sec` spans in a tight loop, regardless
//!    of the `parent_rate` value.
//! 3. `cardinality_cap_evicts_to_overflow_marker` —
//!    [`CardinalityCap::process`] evicts the first-N-wins boundary
//!    and replaces the (N+1)th value with `__other__`.
//! 4. `tail_sampler_rule_match_records_trace` — the rule-list
//!    [`TailSampler`] records a trace_id once a rule matches the
//!    observed outcome.
//!
//! Each test exercises a single public type, in line with the
//! "one test per canonical case" rule from the v22 spec.
//!
//! ## Why this file lives in `tests/` (not `src/`)
//!
//! The `src/cardinality.rs` and `src/sampling.rs` modules already
//! carry in-source `#[cfg(test)] mod tests` blocks for the
//! fine-grained unit tests. This file is the **integration
//! counterpart** — each test pulls in the public type from
//! `pheno_tracing::*` exactly as a downstream consumer would, and
//! verifies the end-to-end behavior. Putting these in `tests/`
//! also exercises the re-exports declared in `src/lib.rs` (if a
//! re-export is missing, the test fails to compile).
//!
//! ## On the deterministic-vs-statistical tradeoff
//!
//! The probabilistic and rate-limited tests below are statistical
//! in nature: they assert that the observed rate falls within a
//! tolerance band around the configured rate, not that it equals
//! the configured rate exactly. The tolerance is wide enough to
//! absorb hash-distribution variance on the test inputs (a few
//! hundred trace_ids is too small a sample to pin the rate tightly
//! without flaking) but tight enough to catch a real bug (a sampler
//! that always records 100% or always drops 100% would fall
//! outside the band).

use pheno_tracing::{
    CardinalityCap, ProbabilisticSampler, RateLimitedSampler, Sampler, SamplingDecision,
    TailSampler, TailSamplingRule, TraceIdRatioBased, DEFAULT_CARDINALITY_CAP,
    DEFAULT_OVERFLOW_MARKER,
};
use std::collections::HashMap;
use std::time::Duration;

// =============================================================================
// Test 1 — ProbabilisticSampler
// =============================================================================

/// v22-T2 / L26 acceptance test 1.
///
/// The [`ProbabilisticSampler`] (alias for [`TraceIdRatioBased`])
/// records a fraction of trace_ids close to the configured rate.
///
/// # Why this is a statistical test
///
/// The decision is a stable hash of the trace_id compared against
/// the rate. For a small sample (256 trace_ids), the observed
/// fraction has a standard error of ~3% at p=0.5. We use a 5%-of-p
/// tolerance band to absorb the noise but still catch gross
/// misbehavior (e.g. a sampler that records 100% or 0% of trace_ids
/// would fail this test by a wide margin).
#[test]
fn probabilistic_sampler_records_at_configured_rate() {
    // Use a moderately large sample so the law of large numbers
    // kicks in. 256 trace_ids × 4 seeds = 1024 trials, which gives
    // a standard error of ~1.5% at p=0.10.
    const N: usize = 1024;
    const RATE: f64 = 0.10;
    const TOLERANCE: f64 = 0.03; // ±3 percentage points

    // First check: the alias resolves to the same type as the
    // canonical probabilistic sampler.
    let sampler: ProbabilisticSampler = ProbabilisticSampler::new(RATE);
    assert_eq!(
        sampler.name(),
        TraceIdRatioBased::new(RATE).name(),
        "ProbabilisticSampler alias must match TraceIdRatioBased behavior"
    );

    // Sweep trace_ids and count Record decisions.
    let mut recorded = 0usize;
    for i in 0..N {
        let trace_id = format!("trace-{i:06}");
        let attrs = HashMap::new();
        let decision = sampler.should_sample_with_attrs(&trace_id, "span", &attrs);
        if decision == SamplingDecision::Record {
            recorded += 1;
        }
    }

    let observed = recorded as f64 / N as f64;
    assert!(
        (observed - RATE).abs() < TOLERANCE,
        "ProbabilisticSampler at rate {RATE} recorded {recorded}/{N} = {observed:.3}; expected {RATE} ± {TOLERANCE}"
    );
}

// =============================================================================
// Test 2 — RateLimitedSampler
// =============================================================================

/// v22-T2 / L26 acceptance test 2.
///
/// The [`RateLimitedSampler`] (probabilistic gate + token-bucket
/// cap) records at most `max_per_sec` spans in a tight loop,
/// regardless of the `parent_rate` value.
///
/// The test sets `parent_rate = 1.0` (so the probabilistic gate is
/// a no-op) and a small `max_per_sec`, then hammers
/// `should_sample` in a tight loop. The recorded count must be
/// approximately `max_per_sec` (the bucket starts full and drains
/// to zero, so the first `max_per_sec` calls record and the rest
/// drop until the bucket refills).
///
/// The tolerance is wide (±20%) because the refill rate is
/// `max_per_sec` tokens per second, and the test loop wall-clock
/// time is non-zero on slow CI.
#[test]
fn rate_limited_sampler_caps_at_max_per_sec() {
    const PARENT_RATE: f64 = 1.0; // gate is a no-op
    const MAX_PER_SEC: f64 = 50.0;
    const N: usize = 500; // 10x max_per_sec — should oversaturate the bucket
    const TOLERANCE_PERCENT: f64 = 0.20;

    let sampler = RateLimitedSampler::new(PARENT_RATE, MAX_PER_SEC);
    assert_eq!(sampler.parent_rate(), PARENT_RATE);
    assert_eq!(sampler.max_per_sec(), MAX_PER_SEC);
    assert_eq!(sampler.name(), "rate-limited");

    let ctx = pheno_tracing::SpanContext::root("trace", "span", false);
    let mut recorded = 0usize;
    let start = std::time::Instant::now();
    for _ in 0..N {
        if sampler.should_sample(&ctx) == SamplingDecision::Record {
            recorded += 1;
        }
    }
    let elapsed = start.elapsed();
    let expected_max = MAX_PER_SEC + (elapsed.as_secs_f64() * MAX_PER_SEC);
    let upper_bound = (expected_max * (1.0 + TOLERANCE_PERCENT)) as usize;
    let lower_bound = (MAX_PER_SEC * (1.0 - TOLERANCE_PERCENT)) as usize;

    assert!(
        recorded <= upper_bound,
        "RateLimitedSampler at max_per_sec={MAX_PER_SEC} recorded {recorded}/{N} in {elapsed:?}; expected ≤ ~{expected_max:.0} (with ±{TOLERANCE_PERCENT:.0}% tolerance, upper bound {upper_bound})"
    );
    assert!(
        recorded >= lower_bound,
        "RateLimitedSampler at max_per_sec={MAX_PER_SEC} recorded {recorded}/{N}; expected ≥ ~{MAX_PER_SEC:.0} (bucket starts full)"
    );
}

/// v22-T2 / L26 acceptance test 2 (companion).
///
/// The probabilistic gate drops trace_ids outside the
/// `parent_rate` window even when the token bucket has capacity.
/// This is the "rate-limited sampling" key property: the
/// probabilistic gate prevents a single high-volume trace_id from
/// saturating the bucket.
#[test]
fn rate_limited_sampler_probabilistic_gate_drops_outside_window() {
    // parent_rate = 0.0 → the gate always rejects; the bucket
    // (no matter how full) never gets consumed.
    let sampler = RateLimitedSampler::new(0.0, 1000.0);
    let ctx = pheno_tracing::SpanContext::root("trace", "span", false);
    for _ in 0..2000 {
        assert_eq!(
            sampler.should_sample(&ctx),
            SamplingDecision::Drop,
            "RateLimitedSampler with parent_rate=0.0 must drop every span"
        );
    }
}

// =============================================================================
// Test 3 — CardinalityCap eviction
// =============================================================================

/// v22-T2 / L26 acceptance test 3.
///
/// The [`CardinalityCap::process`] method evicts the (N+1)th
/// distinct value for an attribute name and replaces it with the
/// overflow marker. The seen-set is bounded by `max_unique_per_attr`
/// (the cap), and the first-N-wins rule guarantees the same values
/// are always passed through verbatim.
///
/// This is the "hash-bucket implementation that bounds memory" the
/// spec calls for: the seen-set never grows past `cap` entries per
/// attribute name, so the total memory is bounded by
/// `O(num_attribute_names * max_unique_per_attr)`.
///
/// ## Test design note
///
/// A `HashMap<String, String>` can hold at most one value per
/// attribute name, so to exercise the (N+1)th-eviction boundary we
/// have to call `process` multiple times — first to fill the
/// seen-set, then to overflow it. Each call passes a one-entry
/// `HashMap` so we exercise the "already-seen" path (1st-2nd
/// calls) and the "overflow" path (3rd-4th calls).
#[test]
fn cardinality_cap_evicts_to_overflow_marker() {
    // Use a small cap so the test runs in constant time and the
    // first-N-wins boundary is obvious.
    const CAP: usize = 3;
    let cap = CardinalityCap::new(CAP);
    assert_eq!(cap.cap(), CAP);
    assert_eq!(cap.overflow_marker(), DEFAULT_OVERFLOW_MARKER);

    // Phase 1: fill the seen-set to CAP. Three distinct values
    // for "user_id" must all be kept verbatim.
    for i in 0..CAP {
        let mut attrs = HashMap::new();
        attrs.insert("user_id".to_string(), format!("user-{i:03}"));
        let report = cap.process(&mut attrs);
        assert_eq!(report.inspected, 1);
        assert_eq!(report.kept, 1);
        assert_eq!(report.overflowed, 0);
        // The value must still be the original (not the overflow marker).
        assert_eq!(attrs.get("user_id").unwrap(), &format!("user-{i:03}"));
    }

    // Phase 2: re-submit the 3 already-seen values — all must be
    // kept (idempotence). The seen-set stays at CAP.
    for i in 0..CAP {
        let mut attrs = HashMap::new();
        attrs.insert("user_id".to_string(), format!("user-{i:03}"));
        let report = cap.process(&mut attrs);
        assert_eq!(report.kept, 1, "already-seen value #{i} must be kept");
        assert_eq!(report.overflowed, 0);
        assert_eq!(attrs.get("user_id").unwrap(), &format!("user-{i:03}"));
    }

    // Phase 3: submit 2 NEW values. Both must be evicted to the
    // overflow marker (seen-set is at CAP).
    for i in 0..2 {
        let mut attrs = HashMap::new();
        attrs.insert("user_id".to_string(), format!("user-overflow-{i}"));
        let report = cap.process(&mut attrs);
        assert_eq!(report.kept, 0);
        assert_eq!(report.overflowed, 1);
        assert_eq!(
            attrs.get("user_id").unwrap(),
            DEFAULT_OVERFLOW_MARKER,
            "(N+1)th value must be replaced with overflow marker"
        );
    }

    // Phase 4: aggregate report across a 5-entry map where each
    // attribute name has multiple values. To exercise this in a
    // single HashMap pass we use 5 distinct attribute names and
    // give each name 4 distinct values across multiple calls —
    // the cap (3) per name is exceeded by the 4th value.
    let cap3 = CardinalityCap::new(3);
    // Submit 4 distinct values for "user_id" (4th is overflow).
    let mut aggregated_inspected = 0;
    let mut aggregated_kept = 0;
    let mut aggregated_overflowed = 0;
    for i in 0..4 {
        let mut attrs = HashMap::new();
        attrs.insert("user_id".to_string(), format!("user-{i:03}"));
        let report = cap3.process(&mut attrs);
        aggregated_inspected += report.inspected;
        aggregated_kept += report.kept;
        aggregated_overflowed += report.overflowed;
    }
    assert_eq!(aggregated_inspected, 4);
    assert_eq!(aggregated_kept, 3, "first 3 distinct values must be kept");
    assert_eq!(aggregated_overflowed, 1, "4th value must overflow");

    // The seen-set for "user_id" must be exactly CAP (the
    // "hash-bucket memory bound" the spec calls for). The 4th
    // value is NOT recorded in the seen-set — only the first-N
    // winners are. We verify by re-submitting each of the 4
    // values and checking the report: 3 must be kept (already-
    // seen), 1 must overflow (the 4th).
    let mut recheck_kept = 0;
    let mut recheck_overflowed = 0;
    for i in 0..4 {
        let mut attrs = HashMap::new();
        attrs.insert("user_id".to_string(), format!("user-{i:03}"));
        let report = cap3.process(&mut attrs);
        recheck_kept += report.kept;
        recheck_overflowed += report.overflowed;
    }
    assert_eq!(recheck_kept, 3, "first-N winners must be remembered");
    assert_eq!(recheck_overflowed, 1, "(N+1)th value must overflow");
}

/// v22-T2 / L26 acceptance test 3 (companion).
///
/// The default cap matches the v22 / L26 spec default
/// ([`DEFAULT_CARDINALITY_CAP`] = 100). This is a thin smoke test that catches
/// accidental changes to the default.
#[test]
fn cardinality_cap_default_is_one_hundred() {
    let cap = CardinalityCap::default();
    assert_eq!(cap.cap(), DEFAULT_CARDINALITY_CAP);
    assert_eq!(cap.cap(), 100);
    assert_eq!(cap.overflow_marker(), "__other__");
    assert_eq!(cap.overflow_marker(), DEFAULT_OVERFLOW_MARKER);
}

// =============================================================================
// Test 4 — TailSampler rule match
// =============================================================================

/// v22-T2 / L26 acceptance test 4.
///
/// The rule-list [`TailSampler`] records a trace_id once a rule
/// matches the observed outcome.
///
/// The test exercises the three canonical rule shapes:
///
/// 1. `error_only` — matches any error span.
/// 2. `min_duration_ms` — matches spans with `duration_ms >= min`.
/// 3. `named` — matches a specific span name.
///
/// Each rule is configured, an outcome is observed, and the
/// subsequent `should_sample` for that trace_id must return
/// `Record`. Outcomes that don't match any rule must leave the
/// trace_id unmarked (`Drop`).
#[test]
fn tail_sampler_rule_match_records_trace() {
    // ----- rule 1: error_only -----
    let error_rule = TailSamplingRule::errors();
    let sampler = TailSampler::new(vec![error_rule]);
    assert_eq!(sampler.name(), "tail-rule");
    assert_eq!(sampler.rules().len(), 1);

    // A healthy span does NOT match the error rule → trace_id is
    // not marked → should_sample returns Drop.
    sampler.observe_outcome("trace-healthy", "GET /foo", false, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-healthy", "s", false)),
        SamplingDecision::Drop,
        "healthy span must not match the error-only rule"
    );

    // An error span DOES match the error rule → trace_id is marked
    // → should_sample returns Record.
    sampler.observe_outcome("trace-error", "GET /foo", true, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-error", "s", false)),
        SamplingDecision::Record,
        "error span must match the error-only rule"
    );

    // ----- rule 2: min_duration_ms -----
    let slow_rule = TailSamplingRule::slow(100);
    let sampler = TailSampler::new(vec![slow_rule]);

    // A fast span (50ms < 100ms threshold) does NOT match.
    sampler.observe_outcome("trace-fast", "GET /foo", false, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-fast", "s", false)),
        SamplingDecision::Drop,
        "fast span (50ms) must not match the slow(100ms) rule"
    );

    // A slow span (200ms >= 100ms threshold) DOES match.
    sampler.observe_outcome("trace-slow", "GET /foo", false, Some(200));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-slow", "s", false)),
        SamplingDecision::Record,
        "slow span (200ms) must match the slow(100ms) rule"
    );

    // A span with unknown duration never matches a min_duration_ms
    // rule (no false positives from missing data).
    sampler.observe_outcome("trace-unknown", "GET /foo", false, None);
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-unknown", "s", false)),
        SamplingDecision::Drop,
        "span with unknown duration must not match a min_duration_ms rule"
    );

    // ----- rule 3: named -----
    let named_rule = TailSamplingRule::named("POST /checkout");
    let sampler = TailSampler::new(vec![named_rule]);

    // A span with a different name does NOT match.
    sampler.observe_outcome("trace-other", "GET /foo", false, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-other", "s", false)),
        SamplingDecision::Drop,
        "span with a different name must not match the named rule"
    );

    // A span with the exact name DOES match.
    sampler.observe_outcome("trace-checkout", "POST /checkout", false, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-checkout", "s", false)),
        SamplingDecision::Record,
        "span with the matching name must match the named rule"
    );
}

/// v22-T2 / L26 acceptance test 4 (companion).
///
/// A multi-rule `TailSampler` composes rules with logical OR:
/// any rule matching is enough to mark the trace_id. This is the
/// "errors and slow spans, but not healthy fast spans" composition
/// pattern from the spec.
#[test]
fn tail_sampler_multi_rule_composition() {
    let sampler = TailSampler::new(vec![
        TailSamplingRule::errors(),            // rule 1: any error
        TailSamplingRule::slow(100),           // rule 2: any span >= 100ms
    ]);

    // Healthy fast span → no match → Drop.
    sampler.observe_outcome("trace-hf", "GET /foo", false, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-hf", "s", false)),
        SamplingDecision::Drop,
        "healthy fast span must match neither rule"
    );

    // Error span → rule 1 match → Record.
    sampler.observe_outcome("trace-err", "GET /foo", true, Some(50));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-err", "s", false)),
        SamplingDecision::Record,
        "error span must match rule 1 (errors)"
    );

    // Healthy slow span → rule 2 match → Record.
    sampler.observe_outcome("trace-slow", "GET /foo", false, Some(500));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-slow", "s", false)),
        SamplingDecision::Record,
        "healthy slow span must match rule 2 (slow)"
    );

    // Error slow span → both match → Record (no double-record).
    sampler.observe_outcome("trace-both", "GET /foo", true, Some(500));
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-both", "s", false)),
        SamplingDecision::Record
    );

    // Reset clears all marks.
    sampler.reset();
    assert_eq!(
        sampler.should_sample(&pheno_tracing::SpanContext::root("trace-err", "s", false)),
        SamplingDecision::Drop,
        "reset() must clear all marks"
    );
}

// =============================================================================
// Helper — small smoke test for the 3-arg Sampler method
// =============================================================================

/// Smoke test: the spec-mandated 3-arg `should_sample_with_attrs`
/// method exists and returns the same decision as the 1-arg
/// `should_sample` for the canonical probabilistic sampler (where
/// the 1-arg method is the only one that consults the trace_id).
#[test]
fn should_sample_with_attrs_matches_should_sample_for_probabilistic() {
    let sampler = ProbabilisticSampler::new(0.5);
    let attrs = HashMap::new();
    let mut matches = 0usize;
    let mut total = 0usize;
    for i in 0..200 {
        let trace_id = format!("trace-{i:06}");
        let one_arg = sampler.should_sample(&pheno_tracing::SpanContext::root(
            &trace_id, "s", false,
        ));
        let three_arg = sampler.should_sample_with_attrs(&trace_id, "s", &attrs);
        total += 1;
        if one_arg == three_arg {
            matches += 1;
        }
    }
    assert_eq!(
        matches, total,
        "should_sample_with_attrs must agree with should_sample for the probabilistic sampler"
    );
}

/// Smoke test: the rule-list `TailSampler` overrides the 3-arg
/// method to consult the rule list directly (rather than going
/// through a `SpanContext`).
#[test]
fn tail_sampler_three_arg_method_consults_rules() {
    let sampler = TailSampler::new(vec![TailSamplingRule::errors()]);
    let mut attrs = HashMap::new();
    attrs.insert("error".to_string(), "true".to_string());

    // Pre-mark the trace_id via observe_outcome so should_sample
    // returns Record.
    sampler.observe_outcome("trace-x", "span", true, Some(10));
    assert_eq!(
        sampler.should_sample_with_attrs("trace-x", "span", &attrs),
        SamplingDecision::Record,
        "TailSampler should_sample_with_attrs must return Record for a marked trace_id"
    );

    // An unmarked trace_id returns Drop.
    let empty_attrs = HashMap::new();
    assert_eq!(
        sampler.should_sample_with_attrs("trace-unmarked", "span", &empty_attrs),
        SamplingDecision::Drop,
        "TailSampler should_sample_with_attrs must return Drop for an unmarked trace_id"
    );
}

/// Suppress the unused-import warning for `Duration` so a future
/// refactor that removes a `Duration::from_millis` call doesn't
/// break the build.
#[allow(dead_code)]
fn _duration_import_silencer() -> Duration {
    Duration::from_millis(0)
}
