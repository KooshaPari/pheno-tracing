//! Sampling-policy port for distributed tracing.
//!
//! Per ADR-036, `pheno-tracing` is the canonical tracing substrate; this
//! module adds the **sampling-policy port** — the part that decides
//! whether a given span gets recorded at the source (head-based) or after
//! the span completes (tail-based). The port is defined by the
//! [`SamplingPolicy`] trait, which adapters consult to make per-span
//! decisions without baking a specific strategy into the call graph.
//!
//! Three strategies ship in-tree:
//!
//! - [`ParentBasedSampler`] — W3C/OTel-default; if the parent context is
//!   sampled, sample; otherwise drop. Implements the "respect upstream
//!   intent" rule from W3C Trace Context §3.
//! - [`RateLimitSampler`] — Bernoulli probabilistic sampler at a
//!   configurable rate (in `[0.0, 1.0]`); useful for high-throughput
//!   services that want a fixed sample ratio without a token-bucket
//!   state machine.
//! - [`TailBasedSampler`] — defers the decision until span end; records
//!   when the recent error rate exceeds a threshold.
//!
//! One combinator ships in-tree:
//!
//! - [`CompositeSampler`] — combines multiple [`SamplingPolicy`]
//!   instances under a [`CompositeMode`] (Any / All / FirstRecord).
//!
//! # When to use
//!
//! - You want a single trait surface so adapters can swap sampling
//!   logic without touching the call graph.
//! - You need explicit control over what gets recorded (vs. relying on
//!   defaults that may oversample or undersample in production).
//! - You want a combinator that lets the call site apply a policy
//!   hierarchy (e.g. "parent-based UNLESS rate-limited").
//!
//! # When NOT to use
//!
//! - You need vendor-specific adaptive sampling (e.g. Honeycomb
//!   `Refinery`, Datadog `Dynamic Sampler`) → depend on a vendor SDK
//!   directly; this module is the fleet-port contract.
//! - You need a token-bucket budget (e.g. "no more than 1000 spans per
//!   second, no exceptions") → bring a dedicated `governor`-based
//!   sampler; [`RateLimitSampler`] is **probabilistic**, not budgeted.
//!
//! # Sampling decision semantics
//!
//! A [`SamplingDecision`] has three outcomes. The semantic difference
//! between `Record` and `RecordAndSample` follows W3C Trace Context §3
//! and the OTel SDK spec:
//!
//! | Variant             | Record this span? | Propagate sampled bit to children? |
//! |---------------------|-------------------|-----------------------------------|
//! | `Drop`              | no                | n/a                               |
//! | `Record`            | yes               | no                                |
//! | `RecordAndSample`   | yes               | yes                               |
//!
//! Use `Record` when this span is interesting in isolation but its
//! subtree is not (e.g. an error report from a leaf service). Use
//! `RecordAndSample` when this span is the trace root and you want
//! downstream children to be sampled too.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

// =============================================================================
// SpanContext — minimal handle used by SamplingPolicy
// =============================================================================

/// Minimal span context consulted by [`SamplingPolicy`] adapters.
///
/// Defined here (rather than in `port.rs`) so the [`SamplingPolicy`]
/// trait does not pull in the heavier [`crate::port::TraceOperation`]
/// shape. Adapters that already have a [`crate::port::TraceId`] /
/// [`crate::port::SpanId`] can build a `SpanContext` from those fields
/// without further mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpanContext {
    /// 128-bit trace identifier (32 lowercase hex chars in W3C format).
    pub trace_id: String,
    /// 64-bit span identifier (16 lowercase hex chars in W3C format).
    pub span_id: String,
    /// W3C trace-flags byte; bit 0 is the "sampled" bit.
    pub trace_flags: u8,
    /// Optional parent context — present when this span is a child of
    /// an upstream trace.
    pub parent: Option<Box<SpanContext>>,
}

impl SpanContext {
    /// Construct a root (no-parent) `SpanContext`.
    ///
    /// `sampled = true` sets the W3C sampled bit (0x01); `sampled = false`
    /// leaves it clear.
    pub fn root(trace_id: impl Into<String>, span_id: impl Into<String>, sampled: bool) -> Self {
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            trace_flags: if sampled { 0x01 } else { 0x00 },
            parent: None,
        }
    }

    /// Attach a parent context, returning a child.
    pub fn with_parent(mut self, parent: SpanContext) -> Self {
        self.parent = Some(Box::new(parent));
        self
    }

    /// True if the sampled bit (bit 0 of `trace_flags`) is set, or if
    /// any ancestor has the sampled bit set (recursive).
    pub fn is_sampled(&self) -> bool {
        if self.trace_flags & 0x01 == 0x01 {
            return true;
        }
        match &self.parent {
            Some(p) => p.is_sampled(),
            None => false,
        }
    }
}

// =============================================================================
// SamplingDecision
// =============================================================================

/// Decision returned by a [`SamplingPolicy::should_sample`] call.
///
/// See the module-level docs for the semantic difference between
/// `Record` and `RecordAndSample`. The short version: `Record` keeps
/// the span only; `RecordAndSample` keeps the span AND propagates the
/// W3C sampled bit to children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SamplingDecision {
    /// Discard the span at the source.
    Drop,
    /// Keep the span — record it and forward to exporters. Do NOT
    /// propagate the sampled bit to children.
    Record,
    /// Keep the span AND propagate the sampled bit to children. This
    /// is the W3C/OTel "sampled" flag semantics: a root decision that
    /// says "trace this and everything downstream".
    RecordAndSample,
}

impl SamplingDecision {
    /// True if the decision is `Record` or `RecordAndSample`
    /// (i.e. the span should be kept). Equivalent to "non-Drop".
    pub fn is_record(self) -> bool {
        matches!(self, Self::Record | Self::RecordAndSample)
    }

    /// True if the decision propagates the sampled bit to children
    /// (i.e. is `RecordAndSample`).
    pub fn is_sampling(self) -> bool {
        matches!(self, Self::RecordAndSample)
    }
}

// =============================================================================
// SamplingPolicy — the port trait
// =============================================================================

/// Port trait for sampling strategies (v12-04 sampling-policy port).
///
/// Every sampling strategy — parent-based, rate-limit, tail-based, vendor
/// adaptive, or any user-defined strategy — implements this trait.
/// Adapters consult a single `dyn SamplingPolicy` to decide per-span,
/// keeping the call graph independent of the active strategy.
///
/// This trait is intentionally minimal: it exposes only the
/// decision-side method. Strategies that need to observe span outcomes
/// (e.g. [`TailBasedSampler`]) expose additional methods on the concrete
/// type, not on the trait, so the port surface stays small and the
/// trait remains trivially object-safe.
pub trait SamplingPolicy: Send + Sync + std::fmt::Debug {
    /// Decide whether a single span should be recorded.
    ///
    /// `span` is the span context at decision time. For head-based
    /// samplers (parent-based, rate-limit) the parent fields are
    /// sufficient. For tail-based samplers the implementation may also
    /// observe the eventual outcome via a concrete-type method
    /// (e.g. [`TailBasedSampler::observe`]) and use that to influence
    /// the next `should_sample` call.
    fn should_sample(&self, span: &SpanContext) -> SamplingDecision;
}

// =============================================================================
// ParentBasedSampler
// =============================================================================

/// Sampler that honors the parent's decision.
///
/// Per W3C Trace Context §3 and the OTel SDK spec: if any ancestor span
/// (or the span itself) has the sampled bit set, return
/// [`SamplingDecision::RecordAndSample`]; otherwise
/// [`SamplingDecision::Drop`]. This is the recommended default for
/// services that participate in a distributed trace — it preserves
/// whatever sampling intent the upstream caller chose.
///
/// # When to use
///
/// - The service sits in the middle of a distributed trace and should
///   respect whatever the upstream caller (gateway, ingress, etc.)
///   chose.
/// - You want a deterministic, parent-driven decision (no probabilistic
///   randomness, no rate budgeting).
///
/// # When NOT to use
///
/// - You are the trace root and have no upstream caller to defer to —
///   pick [`RateLimitSampler`] or [`TailBasedSampler`] instead.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParentBasedSampler;

impl ParentBasedSampler {
    /// Construct a new parent-based sampler.
    pub fn new() -> Self {
        Self
    }
}

impl SamplingPolicy for ParentBasedSampler {
    fn should_sample(&self, span: &SpanContext) -> SamplingDecision {
        if span.is_sampled() {
            // Propagate the sampled bit so downstream children are also
            // sampled (W3C Trace Context §3.2.2.4).
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        }
    }
}

// =============================================================================
// RateLimitSampler — Bernoulli probabilistic sampler
// =============================================================================

/// Bernoulli probabilistic sampler at a fixed rate.
///
/// On each call, generates a deterministic pseudo-random value from the
/// span's hash. If the value is below `rate`, returns
/// [`SamplingDecision::RecordAndSample`]; otherwise
/// [`SamplingDecision::Drop`]. The decision is deterministic per
/// `(rate, span)` pair, so the same span always makes the same
/// decision — important for tail consistency in distributed traces.
///
/// The hashing uses [`DefaultHasher`] (SipHash in std); it is not
/// cryptographic but is well-distributed enough for sampling purposes.
/// No external `rand` dependency is required.
///
/// # When to use
///
/// - You are the trace root and want a fixed sample ratio (e.g. 10% of
///   incoming requests).
/// - You want per-call rate limiting without a token-bucket
///   state-machine.
///
/// # When NOT to use
///
/// - You need a strict N-per-second budget (use a token-bucket
///   implementation instead, e.g. `governor`).
/// - You are downstream of a sampled trace and want to honor the
///   caller's decision (use [`ParentBasedSampler`]).
#[derive(Debug, Clone, Copy)]
pub struct RateLimitSampler {
    /// Sample rate in `[0.0, 1.0]`. 0.0 = drop everything, 1.0 = record
    /// everything.
    rate: f64,
}

impl RateLimitSampler {
    /// Construct a probabilistic sampler with the given rate (in
    /// `[0.0, 1.0]`). Panics if `rate` is outside this range.
    pub fn new(rate: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&rate),
            "rate must be in [0.0, 1.0], got {rate}"
        );
        Self { rate }
    }

    /// Convenience: rate 0.0 (drop every span).
    pub fn never() -> Self {
        Self::new(0.0)
    }

    /// Convenience: rate 1.0 (record every span, with `RecordAndSample`).
    pub fn always() -> Self {
        Self::new(1.0)
    }

    /// Return the configured rate.
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Hash-based Bernoulli decision. Deterministic per
    /// `(rate, span)` pair so the same span always makes the same call.
    fn decide(&self, span: &SpanContext) -> SamplingDecision {
        if self.rate == 0.0 {
            return SamplingDecision::Drop;
        }
        if self.rate == 1.0 {
            return SamplingDecision::RecordAndSample;
        }
        let mut hasher = DefaultHasher::new();
        span.hash(&mut hasher);
        let h = hasher.finish();
        // Map to [0.0, 1.0) (u64::MAX is odd, so division is fine).
        let p = (h as f64) / (u64::MAX as f64);
        if p < self.rate {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        }
    }
}

impl SamplingPolicy for RateLimitSampler {
    fn should_sample(&self, span: &SpanContext) -> SamplingDecision {
        self.decide(span)
    }
}

// =============================================================================
// TailBasedSampler — defers decision until span end
// =============================================================================

/// Tail-based sampler that records spans when the recent error rate
/// exceeds a threshold.
///
/// The sampler keeps a sliding window of the last `window_size` span
/// outcomes (each marked `was_error: bool`). On each
/// [`SamplingPolicy::should_sample`] call it returns
/// [`SamplingDecision::Drop`] unless a previous [`Self::observe`] call
/// armed the sampler (i.e. the error rate crossed the threshold). When
/// armed, the next `should_sample` call returns
/// [`SamplingDecision::Record`] (single-shot, then disarms).
///
/// This is a deliberately simple implementation — no percentile
/// tracking, no per-route budgets — but it covers the most common
/// tail-sampling use case (capture error bursts, ignore healthy
/// traffic).
///
/// # When to use
///
/// - You have high-throughput spans where head-based sampling would
///   miss rare error bursts (e.g. 1-in-10000 errors that a 10% sampler
///   would skip).
/// - You can defer the sampling decision until span end (i.e. the
///   exporter buffers spans briefly before forwarding).
///
/// # When NOT to use
///
/// - You need real-time forwarding (every span goes out as soon as it
///   ends) — use [`ParentBasedSampler`] or [`RateLimitSampler`].
/// - You need per-route or per-tenant policies — extend the
///   `should_sample` signature.
#[derive(Debug)]
pub struct TailBasedSampler {
    /// Window size (number of recent observations).
    window_size: usize,
    /// Error rate threshold in `[0.0, 1.0]`; above this, arm the sampler.
    error_threshold: f64,
    /// Sliding window of (was_error) outcomes, newest at the end.
    window: Mutex<Vec<bool>>,
    /// True when the error rate crossed the threshold; the next
    /// `should_sample` call returns `Record`, then this clears.
    armed: Mutex<bool>,
}

impl TailBasedSampler {
    /// Construct a tail-based sampler with default window size (100)
    /// and default error threshold (0.10 = 10%).
    pub fn new() -> Self {
        Self::with_params(100, 0.10)
    }

    /// Construct a tail-based sampler with explicit window and
    /// threshold. `window_size` must be > 0; `error_threshold` must be
    /// in `[0.0, 1.0]`.
    pub fn with_params(window_size: usize, error_threshold: f64) -> Self {
        assert!(window_size > 0, "window_size must be > 0");
        assert!(
            (0.0..=1.0).contains(&error_threshold),
            "error_threshold must be in [0.0, 1.0]"
        );
        Self {
            window_size,
            error_threshold,
            window: Mutex::new(Vec::with_capacity(window_size)),
            armed: Mutex::new(false),
        }
    }

    /// Inform the sampler of a span's eventual outcome. If the running
    /// error rate (in the current window) crosses `error_threshold`,
    /// the sampler arms itself so the next `should_sample` call
    /// returns `Record`.
    ///
    /// Concrete-type method (not on the [`SamplingPolicy`] trait) so
    /// the port surface stays small.
    pub fn observe(&self, _span: &SpanContext, was_error: bool) {
        let mut window = self.window.lock().unwrap();
        if window.len() >= self.window_size {
            window.remove(0);
        }
        window.push(was_error);

        let errors = window.iter().filter(|e| **e).count();
        let rate = errors as f64 / window.len().max(1) as f64;
        if rate > self.error_threshold {
            let mut armed = self.armed.lock().unwrap();
            *armed = true;
        }
    }

    /// Return the current window size (number of recorded observations).
    /// Useful for tests and observability.
    pub fn current_window_len(&self) -> usize {
        self.window.lock().unwrap().len()
    }
}

impl Default for TailBasedSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplingPolicy for TailBasedSampler {
    fn should_sample(&self, _span: &SpanContext) -> SamplingDecision {
        let mut armed = self.armed.lock().unwrap();
        if *armed {
            *armed = false;
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }
}

// =============================================================================
// CompositeSampler — combinator
// =============================================================================

/// How [`CompositeSampler`] combines multiple [`SamplingPolicy`]
/// decisions on a single span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeMode {
    /// `Record` (or stronger) if ANY policy returns a non-Drop decision;
    /// the strongest decision wins (`RecordAndSample` > `Record` >
    /// `Drop`). If ALL policies return `Drop`, returns `Drop`.
    Any,
    /// `RecordAndSample` if ALL policies return a non-Drop decision;
    /// `Drop` if ANY policy returns `Drop`. Among non-Drop decisions,
    /// the weakest wins (`Record` < `RecordAndSample`).
    All,
    /// First non-`Drop` decision wins (in policy order); if ALL return
    /// `Drop`, returns `Drop`.
    FirstRecord,
}

impl CompositeMode {
    /// Stable, human-readable name.
    pub fn name(self) -> &'static str {
        match self {
            CompositeMode::Any => "any",
            CompositeMode::All => "all",
            CompositeMode::FirstRecord => "first-record",
        }
    }
}

/// Combinator sampler that runs every span through multiple
/// [`SamplingPolicy`] instances and combines the results using a
/// [`CompositeMode`].
///
/// # When to use
///
/// - You want a hierarchical policy (e.g. "parent-based UNLESS the
///   rate-limit is exhausted, in which case drop"). Use
///   [`CompositeMode::FirstRecord`] with the loosest policy first.
/// - You want a quorum-style policy (e.g. "record only if both
///   parent-based AND rate-limit say yes"). Use
///   [`CompositeMode::All`].
/// - You want to A/B test policies in production (run two policies
///   side-by-side, take the strongest). Use [`CompositeMode::Any`].
///
/// # When NOT to use
///
/// - A single policy is sufficient (adding more policies increases the
///   per-span decision cost — each `should_sample` call iterates
///   every policy).
#[derive(Debug)]
pub struct CompositeSampler {
    policies: Vec<Box<dyn SamplingPolicy>>,
    mode: CompositeMode,
}

impl CompositeSampler {
    /// Construct a composite sampler with no policies. An empty
    /// composite returns [`SamplingDecision::Drop`] for every span;
    /// use [`Self::with_policy`] / [`Self::with_policies`] to add
    /// policies.
    pub fn new(mode: CompositeMode) -> Self {
        Self {
            policies: Vec::new(),
            mode,
        }
    }

    /// Add a single policy. Returns `self` for builder-style chaining.
    pub fn with_policy(mut self, policy: Box<dyn SamplingPolicy>) -> Self {
        self.policies.push(policy);
        self
    }

    /// Add multiple policies at once. Returns `self` for builder-style
    /// chaining.
    pub fn with_policies(mut self, policies: Vec<Box<dyn SamplingPolicy>>) -> Self {
        self.policies.extend(policies);
        self
    }

    /// Return the configured mode.
    pub fn mode(&self) -> CompositeMode {
        self.mode
    }

    /// Return the number of policies in the composite.
    pub fn len(&self) -> usize {
        self.policies.len()
    }

    /// True if the composite has no policies.
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    fn combine(&self, span: &SpanContext) -> SamplingDecision {
        if self.policies.is_empty() {
            return SamplingDecision::Drop;
        }
        match self.mode {
            CompositeMode::Any => {
                // Strongest decision wins.
                let mut best = SamplingDecision::Drop;
                for p in &self.policies {
                    match p.should_sample(span) {
                        SamplingDecision::RecordAndSample => {
                            return SamplingDecision::RecordAndSample
                        }
                        SamplingDecision::Record => best = SamplingDecision::Record,
                        SamplingDecision::Drop => {}
                    }
                }
                best
            }
            CompositeMode::All => {
                // All must say record-or-better; any drop means drop.
                // Among non-Drop, the weakest wins (Record < RecordAndSample).
                let mut weakest = SamplingDecision::RecordAndSample;
                for p in &self.policies {
                    match p.should_sample(span) {
                        SamplingDecision::Drop => return SamplingDecision::Drop,
                        SamplingDecision::RecordAndSample => {}
                        SamplingDecision::Record => weakest = SamplingDecision::Record,
                    }
                }
                weakest
            }
            CompositeMode::FirstRecord => {
                // First non-Drop wins.
                for p in &self.policies {
                    let d = p.should_sample(span);
                    if d.is_record() {
                        return d;
                    }
                }
                SamplingDecision::Drop
            }
        }
    }
}

impl SamplingPolicy for CompositeSampler {
    fn should_sample(&self, span: &SpanContext) -> SamplingDecision {
        self.combine(span)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- helpers -----

    fn sampled_root() -> SpanContext {
        SpanContext::root("trace-1", "span-1", true)
    }

    fn unsampled_root() -> SpanContext {
        SpanContext::root("trace-1", "span-1", false)
    }

    fn child_of(parent: SpanContext) -> SpanContext {
        SpanContext::root("trace-1", "span-child", false).with_parent(parent)
    }

    // ----- SpanContext -----

    #[test]
    fn span_context_root_sets_sampled_bit() {
        assert_eq!(sampled_root().trace_flags, 0x01);
        assert_eq!(unsampled_root().trace_flags, 0x00);
        assert!(unsampled_root().parent.is_none());
    }

    #[test]
    fn span_context_is_sampled_walks_ancestors() {
        let parent = sampled_root();
        let child = child_of(parent);
        assert!(child.is_sampled());
        // A different subtree (no sampled ancestor) is not.
        let orphan = unsampled_root();
        assert!(!orphan.is_sampled());
    }

    #[test]
    fn span_context_with_parent_links_chain() {
        let p = sampled_root();
        let c = child_of(p.clone());
        assert!(c.parent.is_some());
        assert!(p.parent.is_none());
    }

    // ----- SamplingDecision -----

    #[test]
    fn decision_is_record_and_is_sampling_helpers() {
        assert!(!SamplingDecision::Drop.is_record());
        assert!(!SamplingDecision::Drop.is_sampling());
        assert!(SamplingDecision::Record.is_record());
        assert!(!SamplingDecision::Record.is_sampling());
        assert!(SamplingDecision::RecordAndSample.is_record());
        assert!(SamplingDecision::RecordAndSample.is_sampling());
    }

    // ----- ParentBasedSampler -----

    #[test]
    fn parent_based_records_and_sample_when_parent_sampled() {
        let s = ParentBasedSampler::new();
        let child = child_of(sampled_root());
        assert_eq!(s.should_sample(&child), SamplingDecision::RecordAndSample);
    }

    #[test]
    fn parent_based_drops_when_parent_unsampled() {
        let s = ParentBasedSampler::new();
        let child = child_of(unsampled_root());
        assert_eq!(s.should_sample(&child), SamplingDecision::Drop);
    }

    #[test]
    fn parent_based_self_sampled_root() {
        let s = ParentBasedSampler::new();
        assert_eq!(
            s.should_sample(&sampled_root()),
            SamplingDecision::RecordAndSample
        );
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
    }

    #[test]
    fn parent_based_propagates_through_deep_chain() {
        let s = ParentBasedSampler::new();
        let root = sampled_root();
        let middle = child_of(root);
        let leaf = child_of(middle);
        assert_eq!(s.should_sample(&leaf), SamplingDecision::RecordAndSample);
    }

    // ----- RateLimitSampler (Bernoulli) -----

    #[test]
    fn rate_limit_zero_drops_everything() {
        let s = RateLimitSampler::never();
        for i in 0..200 {
            let span = SpanContext::root("t", format!("s{i}"), false);
            assert_eq!(s.should_sample(&span), SamplingDecision::Drop);
        }
    }

    #[test]
    fn rate_limit_one_records_everything() {
        let s = RateLimitSampler::always();
        for i in 0..200 {
            let span = SpanContext::root("t", format!("s{i}"), false);
            assert_eq!(s.should_sample(&span), SamplingDecision::RecordAndSample);
        }
    }

    #[test]
    fn rate_limit_is_deterministic_per_span() {
        let s = RateLimitSampler::new(0.5);
        let span = SpanContext::root("trace-1", "span-1", false);
        let d1 = s.should_sample(&span);
        let d2 = s.should_sample(&span);
        let d3 = s.should_sample(&span);
        assert_eq!(d1, d2);
        assert_eq!(d2, d3);
    }

    #[test]
    fn rate_limit_half_rate_samples_roughly_half() {
        // 10_000 random spans at rate=0.5 — DefaultHasher (SipHash) is
        // well-distributed, so we expect ~50%. Loose bounds
        // (45–55%) to avoid CI flakiness.
        let s = RateLimitSampler::new(0.5);
        let n = 10_000;
        let sampled = (0..n)
            .map(|i| SpanContext::root("trace", format!("s{i}"), false))
            .filter(|span| s.should_sample(span).is_record())
            .count();
        let pct = sampled as f64 / n as f64;
        assert!(
            (0.45..=0.55).contains(&pct),
            "expected ~50% sampled, got {pct:.4} ({sampled}/{n})"
        );
    }

    #[test]
    fn rate_limit_ten_percent_rate() {
        let s = RateLimitSampler::new(0.10);
        let n = 10_000;
        let sampled = (0..n)
            .map(|i| SpanContext::root("trace", format!("s{i}"), false))
            .filter(|span| s.should_sample(span).is_record())
            .count();
        let pct = sampled as f64 / n as f64;
        assert!(
            (0.07..=0.13).contains(&pct),
            "expected ~10% sampled, got {pct:.4} ({sampled}/{n})"
        );
    }

    // ----- TailBasedSampler -----

    #[test]
    fn tail_based_drops_by_default() {
        let s = TailBasedSampler::new();
        let span = unsampled_root();
        assert_eq!(s.should_sample(&span), SamplingDecision::Drop);
    }

    #[test]
    fn tail_based_records_after_threshold_cross() {
        // 100% error rate > 10% threshold → arm.
        let s = TailBasedSampler::with_params(10, 0.10);
        let span = unsampled_root();
        for _ in 0..10 {
            s.observe(&span, true);
        }
        assert_eq!(s.should_sample(&span), SamplingDecision::Record);
    }

    #[test]
    fn tail_based_armed_flag_is_single_shot() {
        // After arming, exactly one Record is returned; subsequent
        // calls return Drop until another arming observation arrives.
        let s = TailBasedSampler::with_params(10, 0.10);
        let span = unsampled_root();
        for _ in 0..10 {
            s.observe(&span, true);
        }
        assert_eq!(s.should_sample(&span), SamplingDecision::Record);
        assert_eq!(s.should_sample(&span), SamplingDecision::Drop);
        assert_eq!(s.should_sample(&span), SamplingDecision::Drop);
    }

    #[test]
    fn tail_based_below_threshold_does_not_arm() {
        // Pre-populate the window with 9 non-errors, then add 1 error.
        // The window becomes [false, false, ..., true] with 1 error in
        // 10 observations → error rate = 0.10 exactly. The arming
        // check is STRICT-greater-than, so 0.10 > 0.10 is false and the
        // sampler must NOT arm. This is the boundary case.
        let s = TailBasedSampler::with_params(10, 0.10);
        let span = unsampled_root();
        for _ in 0..9 {
            s.observe(&span, false); // 0 errors / n → not > 0.10
        }
        s.observe(&span, true); // 1 error / 10 = 0.10 → NOT > 0.10
        assert_eq!(s.should_sample(&span), SamplingDecision::Drop);
    }

    #[test]
    fn tail_based_arms_strictly_above_threshold() {
        // 2 errors in 10 = 0.20 > 0.10 → arm.
        let s = TailBasedSampler::with_params(10, 0.10);
        let span = unsampled_root();
        for _ in 0..8 {
            s.observe(&span, false);
        }
        s.observe(&span, true);
        s.observe(&span, true);
        assert_eq!(s.should_sample(&span), SamplingDecision::Record);
    }

    #[test]
    fn tail_based_window_eviction() {
        // 11 observations in a window of 10 → oldest evicted, latest
        // recorded. Build a 100% error stream in the latest 10 → arm.
        let s = TailBasedSampler::with_params(10, 0.10);
        let span = unsampled_root();
        // First observation is "ok" (will be evicted).
        s.observe(&span, false);
        // Next 10 are errors — at any point after the 10th the
        // window has 10 errors → arm.
        for _ in 0..10 {
            s.observe(&span, true);
        }
        assert_eq!(s.current_window_len(), 10);
        assert_eq!(s.should_sample(&span), SamplingDecision::Record);
    }

    // ----- CompositeSampler -----

    #[test]
    fn composite_empty_always_drops() {
        let s = CompositeSampler::new(CompositeMode::Any);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
        let s = CompositeSampler::new(CompositeMode::All);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
        let s = CompositeSampler::new(CompositeMode::FirstRecord);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
    }

    #[test]
    fn composite_any_takes_strongest() {
        // One policy says Drop, another says RecordAndSample →
        // RecordAndSample (strongest non-Drop wins).
        let drop_pol: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let sample_pol: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let s = CompositeSampler::new(CompositeMode::Any)
            .with_policy(drop_pol)
            .with_policy(sample_pol);
        let span = unsampled_root();
        assert_eq!(s.should_sample(&span), SamplingDecision::RecordAndSample);
    }

    #[test]
    fn composite_any_all_drop_is_drop() {
        let p1: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let p2: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let s = CompositeSampler::new(CompositeMode::Any)
            .with_policy(p1)
            .with_policy(p2);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
    }

    #[test]
    fn composite_any_prefers_record_and_sample_over_record() {
        // One policy says Record, another says RecordAndSample →
        // RecordAndSample wins.
        // Construct by using two parent-based samplers: one for a
        // sampled parent (RecordAndSample) and one for an unsampled
        // context (Drop, not useful). Instead use a determinstic
        // approach: build a tiny custom policy inline.
        struct RecordOnly;
        impl SamplingPolicy for RecordOnly {
            fn should_sample(&self, _span: &SpanContext) -> SamplingDecision {
                SamplingDecision::Record
            }
        }
        impl std::fmt::Debug for RecordOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "RecordOnly")
            }
        }
        struct RecordAndSampleOnly;
        impl SamplingPolicy for RecordAndSampleOnly {
            fn should_sample(&self, _span: &SpanContext) -> SamplingDecision {
                SamplingDecision::RecordAndSample
            }
        }
        impl std::fmt::Debug for RecordAndSampleOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "RecordAndSampleOnly")
            }
        }
        let p1: Box<dyn SamplingPolicy> = Box::new(RecordOnly);
        let p2: Box<dyn SamplingPolicy> = Box::new(RecordAndSampleOnly);
        let s = CompositeSampler::new(CompositeMode::Any)
            .with_policy(p1)
            .with_policy(p2);
        assert_eq!(
            s.should_sample(&unsampled_root()),
            SamplingDecision::RecordAndSample
        );
    }

    #[test]
    fn composite_all_requires_all_to_record() {
        // One policy says Drop → composite says Drop.
        let p_drop: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let p_sample: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let s = CompositeSampler::new(CompositeMode::All)
            .with_policy(p_drop)
            .with_policy(p_sample);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
        // Both sample → composite says RecordAndSample.
        let p1: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let p2: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let s = CompositeSampler::new(CompositeMode::All)
            .with_policy(p1)
            .with_policy(p2);
        assert_eq!(
            s.should_sample(&unsampled_root()),
            SamplingDecision::RecordAndSample
        );
    }

    #[test]
    fn composite_all_prefers_record_over_record_and_sample() {
        // In All mode, the weakest non-Drop wins: Record < RecordAndSample.
        struct RecordOnly;
        impl SamplingPolicy for RecordOnly {
            fn should_sample(&self, _span: &SpanContext) -> SamplingDecision {
                SamplingDecision::Record
            }
        }
        impl std::fmt::Debug for RecordOnly {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "RecordOnly")
            }
        }
        let p1: Box<dyn SamplingPolicy> = Box::new(RecordOnly);
        let p2: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let s = CompositeSampler::new(CompositeMode::All)
            .with_policy(p1)
            .with_policy(p2);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Record);
    }

    #[test]
    fn composite_first_record_takes_first_non_drop() {
        // First policy says Drop, second says RecordAndSample →
        // second wins.
        let p_drop: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let p_sample: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::always());
        let s = CompositeSampler::new(CompositeMode::FirstRecord)
            .with_policy(p_drop)
            .with_policy(p_sample);
        assert_eq!(
            s.should_sample(&unsampled_root()),
            SamplingDecision::RecordAndSample
        );
        // All Drop → Drop.
        let p1: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let p2: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::never());
        let s = CompositeSampler::new(CompositeMode::FirstRecord)
            .with_policy(p1)
            .with_policy(p2);
        assert_eq!(s.should_sample(&unsampled_root()), SamplingDecision::Drop);
    }

    #[test]
    fn composite_mode_name_is_stable() {
        assert_eq!(CompositeMode::Any.name(), "any");
        assert_eq!(CompositeMode::All.name(), "all");
        assert_eq!(CompositeMode::FirstRecord.name(), "first-record");
    }

    #[test]
    fn composite_len_and_is_empty() {
        let s = CompositeSampler::new(CompositeMode::Any);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        let s = s.with_policy(Box::new(RateLimitSampler::always()));
        assert!(!s.is_empty());
        assert_eq!(s.len(), 1);
        let s = s.with_policies(vec![Box::new(RateLimitSampler::never())]);
        assert_eq!(s.len(), 2);
    }

    // ----- Trait object safety -----

    #[test]
    fn sampling_policy_is_object_safe() {
        // Compile-time check: the trait can be used as
        // `dyn SamplingPolicy`. If a future refactor accidentally adds
        // a generic method or a `Self: Sized` bound, this test fails
        // to compile.
        fn _accept_dyn(_s: &dyn SamplingPolicy) {}
        _accept_dyn(&ParentBasedSampler::new());
        _accept_dyn(&RateLimitSampler::new(0.5));
        _accept_dyn(&TailBasedSampler::new());

        let _boxed: Vec<Box<dyn SamplingPolicy>> = vec![
            Box::new(ParentBasedSampler::new()),
            Box::new(RateLimitSampler::new(0.5)),
            Box::new(TailBasedSampler::new()),
            Box::new(CompositeSampler::new(CompositeMode::Any)),
        ];
    }

    // ----- Cross-strategy: parent-based upstream + rate-limit downstream -----

    #[test]
    fn composite_mimics_parent_unless_rate_limited() {
        // Build a composite that:
        //   1. FirstRecord: if parent is sampled, record (parent-based)
        //   2. else: rate-limit at 50%
        // This is the canonical "respect upstream intent, but cap at
        // 50% if the upstream didn't sample" pattern.
        let parent_pol: Box<dyn SamplingPolicy> = Box::new(ParentBasedSampler::new());
        let rate_pol: Box<dyn SamplingPolicy> = Box::new(RateLimitSampler::new(0.5));
        let s = CompositeSampler::new(CompositeMode::FirstRecord)
            .with_policy(parent_pol)
            .with_policy(rate_pol);

        // Sampled parent → first policy says RecordAndSample, second
        // is never consulted.
        let sampled_child = child_of(sampled_root());
        assert_eq!(s.should_sample(&sampled_child), SamplingDecision::RecordAndSample);
        // Unsampled parent, fresh span → first policy says Drop, then
        // the rate-limit policy gets consulted.
        // 50% of "trace/x" is recorded; not deterministic per span so
        // just check that we get a non-Drop or Drop (both are valid).
        let s2 = CompositeSampler::new(CompositeMode::FirstRecord)
            .with_policy(Box::new(ParentBasedSampler::new()))
            .with_policy(Box::new(RateLimitSampler::new(0.5)));
        for i in 0..100 {
            let span = SpanContext::root("trace", format!("s{i}"), false);
            let d = s2.should_sample(&span);
            // Both Drop and RecordAndSample are valid; this just
            // exercises the combinator with the canonical shape.
            assert!(
                d == SamplingDecision::Drop || d == SamplingDecision::RecordAndSample,
                "unexpected decision {d:?}"
            );
        }
    }
}
