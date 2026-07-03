//! Sampling primitives for distributed tracing.
//!
//! Per ADR-036, `pheno-tracing` is the canonical tracing substrate; this
//! module adds the **sampling decision** layer — the part that decides
//! whether a given span gets recorded at the source (head-based) or after
//! the span completes (tail-based). Three samplers ship in-tree:
//!
//! - [`ParentBasedSampler`] — W3C/OTel-default; if the parent context is
//!   sampled, sample; otherwise drop. Implements the "respect upstream
//!   intent" rule from W3C Trace Context §3.
//! - [`RateLimitSampler`] — token-bucket sampler; cap at N spans per
//!   second. Useful for high-throughput services where a fixed budget
//!   is preferable to a probabilistic rate.
//! - [`TailBasedSampler`] — observes the recent stream of span outcomes
//!   and records when the error rate exceeds a threshold. Cheap
//!   approximation: a sliding window of (timestamp, was_error) pairs.
//!
//! # When to use
//!
//! - You want a single trait surface so adapters can swap sampling logic
//!   without touching the call graph.
//! - You need explicit control over what gets recorded (vs. relying on
//!   defaults that may oversample or undersample in production).
//!
//! # When NOT to use
//!
//! - You only need "always sample" or "never sample" → construct an
//!   [`AlwaysSampler`] / [`NeverSampler`] inline; no need for this
//!   module.
//! - You need vendor-specific adaptive sampling → depend on a vendor
//!   SDK directly; this module is the fleet-port contract.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

/// Acquire a sampler mutex, recovering from poison if needed (T24 P3 fix).
///
/// On poison, emit a `tracing::warn!` (tagged `pheno_tracing.sampling` for
/// fleet-level filtering) and clear the poison flag so the next `lock()`
/// succeeds. This mirrors the recovery pattern already used by
/// `InMemoryAdapter::submit` in `adapters.rs` and keeps the trace path
/// alive across a panic in an unrelated thread.
///
/// Rationale: a poisoned sampler mutex indicates another thread panicked
/// while holding the lock. The sampler state is local decision logic
/// (token-bucket, sliding window, armed flag); discarding it would break
/// the sampling rate / error-window contract. We log the anomaly and
/// continue with the recovered state so a downstream trace pipeline stays
/// alive across panics in unrelated code.
///
/// `Mutex::clear_poison` was stabilized in Rust 1.77; the crate MSRV is
/// 1.75, so we suppress the clippy `incompatible_msrv` lint locally. The
/// helper itself is only reachable when poison is already detected (i.e.
/// `mutex.lock()` already returned `Err`), so callers on Rust 1.75 would
/// fail at the `clear_poison` call site — bumping MSRV to 1.77 is the
/// proper long-term fix; see the T24 PR body for the deferred bump.
#[allow(clippy::incompatible_msrv)]
#[inline]
fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, what: &'static str) -> MutexGuard<'a, T> {
    if let Ok(g) = mutex.lock() {
        return g;
    }
    // Poison detected — clear the flag and re-lock. Log so operators can
    // spot a thread panic that left the sampler state in an unknown shape.
    tracing::warn!(
        target = "pheno_tracing.sampling",
        "{what}: mutex lock poisoned — recovering (panic in another thread)"
    );
    mutex.clear_poison();
    mutex
        .lock()
        .expect("poison flag just cleared; lock must succeed")
}

// =============================================================================
// SpanContext — minimal handle used by the Sampler trait
// =============================================================================

/// Minimal span context consulted by samplers.
///
/// Defined here (rather than in `port.rs`) so the `Sampler` trait does not
/// pull in the heavier `TraceOperation` shape. Adapters that already have
/// a [`crate::port::TraceId`] / [`crate::port::SpanId`] can build a
/// `SpanContext` from those fields without further mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpanContext {
    /// 128-bit trace identifier as 32 lowercase hex chars.
    pub trace_id: String,
    /// 64-bit span identifier as 16 lowercase hex chars.
    pub span_id: String,
    /// W3C trace-flags byte; bit 0 is the "sampled" bit.
    pub trace_flags: u8,
    /// Optional parent context — present when this span is a child of
    /// an upstream trace.
    pub parent: Option<Box<SpanContext>>,
}

impl SpanContext {
    /// True if the sampled bit (bit 0 of `trace_flags`) is set, or if any
    /// ancestor has the sampled bit set (recursive).
    pub fn is_sampled(&self) -> bool {
        if self.trace_flags & 0x01 == 0x01 {
            return true;
        }
        match &self.parent {
            Some(p) => p.is_sampled(),
            None => false,
        }
    }

    /// Construct a root (no-parent) SpanContext.
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
}

// =============================================================================
// SamplingDecision
// =============================================================================

/// Decision returned by a [`Sampler::should_sample`] call.
///
/// `Record` means the span should be kept and exported; `Drop` means it
/// should be discarded at the source (sampling the trace at the head).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingDecision {
    /// Keep the span — record it and forward to exporters.
    Record,
    /// Discard the span at the source.
    Drop,
}

impl SamplingDecision {
    /// True if the decision is [`SamplingDecision::Record`].
    pub fn is_record(self) -> bool {
        matches!(self, Self::Record)
    }
}

// =============================================================================
// Sampler trait
// =============================================================================

/// Port trait for sampling strategies.
///
/// Every sampling strategy (parent-based, rate-limit, tail-based, vendor
/// adaptive, etc.) implements this trait. Adapters consult a single
/// `dyn Sampler` to decide per-span, keeping the call graph independent of
/// the active strategy.
pub trait Sampler: Send + Sync {
    /// Stable, human-readable name (e.g. `parent-based`, `rate-limit`).
    fn name(&self) -> &str;

    /// Decide whether a single span should be recorded.
    ///
    /// `ctx` is the span context at decision time. For head-based samplers
    /// (parent-based, rate-limit) the parent fields are sufficient; for
    /// tail-based samplers the implementation may also observe the
    /// eventual outcome via [`Sampler::observe`].
    fn should_sample(&self, ctx: &SpanContext) -> SamplingDecision;

    /// Inform the sampler of a span's eventual outcome. Required for
    /// tail-based samplers; head-based samplers can ignore this.
    ///
    /// Default is a no-op so head-based samplers don't have to override.
    fn observe(&self, _ctx: &SpanContext, _was_error: bool) {}
}

// =============================================================================
// AlwaysSampler / NeverSampler — trivial defaults
// =============================================================================

/// Trivial sampler that always records every span.
#[derive(Debug, Default, Clone, Copy)]
pub struct AlwaysSampler;

impl Sampler for AlwaysSampler {
    fn name(&self) -> &str {
        "always"
    }

    fn should_sample(&self, _ctx: &SpanContext) -> SamplingDecision {
        SamplingDecision::Record
    }
}

/// Trivial sampler that drops every span.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverSampler;

impl Sampler for NeverSampler {
    fn name(&self) -> &str {
        "never"
    }

    fn should_sample(&self, _ctx: &SpanContext) -> SamplingDecision {
        SamplingDecision::Drop
    }
}

// =============================================================================
// ParentBasedSampler
// =============================================================================

/// Sampler that honors the parent's decision.
///
/// Per W3C Trace Context §3 and the OTel SDK spec: if any ancestor span
/// (or the span itself) has the sampled bit set, record; otherwise drop.
/// This is the recommended default for services that participate in a
/// distributed trace — it preserves whatever sampling intent the upstream
/// caller chose.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParentBasedSampler;

impl ParentBasedSampler {
    /// Construct a new parent-based sampler.
    pub fn new() -> Self {
        Self
    }
}

impl Sampler for ParentBasedSampler {
    fn name(&self) -> &str {
        "parent-based"
    }

    fn should_sample(&self, ctx: &SpanContext) -> SamplingDecision {
        if ctx.is_sampled() {
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }
}

// =============================================================================
// RateLimitSampler
// =============================================================================

/// Token-bucket sampler that caps at `N` records per second.
///
/// On each `should_sample` call the sampler decrements the bucket; when
/// the bucket is empty, drops are returned until the bucket refills at
/// the configured rate. The bucket is sized at `max_burst` tokens so
/// short bursts above the average rate are tolerated.
#[derive(Debug)]
pub struct RateLimitSampler {
    /// Average records per second.
    rate_per_sec: f64,
    /// Maximum burst size (tokens that can accumulate in idle periods).
    max_burst: f64,
    /// Current bucket level (fractional tokens allowed).
    tokens: Mutex<TokenState>,
}

#[derive(Debug)]
struct TokenState {
    /// Current token count.
    tokens: f64,
    /// Last refill timestamp.
    last_refill: Instant,
}

impl RateLimitSampler {
    /// Construct a rate-limit sampler with the given average rate and
    /// maximum burst. `max_burst` defaults to `rate_per_sec` (1-second
    /// burst).
    pub fn new(rate_per_sec: f64) -> Self {
        Self::with_burst(rate_per_sec, rate_per_sec)
    }

    /// Construct a rate-limit sampler with an explicit burst size.
    pub fn with_burst(rate_per_sec: f64, max_burst: f64) -> Self {
        assert!(rate_per_sec > 0.0, "rate_per_sec must be > 0");
        assert!(max_burst > 0.0, "max_burst must be > 0");
        Self {
            rate_per_sec,
            max_burst,
            tokens: Mutex::new(TokenState {
                tokens: max_burst,
                last_refill: Instant::now(),
            }),
        }
    }

    /// Refill the bucket proportional to elapsed time and try to consume
    /// one token. Returns Record if a token was consumed, Drop otherwise.
    fn try_consume(&self) -> SamplingDecision {
        let mut state = lock_or_recover(&self.tokens, "RateLimitSampler::try_consume");
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill);
        let refill = (elapsed.as_secs_f64()) * self.rate_per_sec;
        state.tokens = (state.tokens + refill).min(self.max_burst);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }
}

impl Sampler for RateLimitSampler {
    fn name(&self) -> &str {
        "rate-limit"
    }

    fn should_sample(&self, _ctx: &SpanContext) -> SamplingDecision {
        self.try_consume()
    }
}

// =============================================================================
// TailBasedSampler
// =============================================================================

/// Tail-based sampler that records spans when the recent error rate
/// exceeds a threshold.
///
/// The sampler keeps a sliding window of the last `window_size` span
/// outcomes (each marked `was_error: bool`). When a new span is observed,
/// the window's error rate is computed; if it exceeds `error_threshold`,
/// the next `should_sample` call returns `Record`. Otherwise `Drop`.
///
/// This is a deliberately simple implementation — no percentile tracking,
/// no per-route budgets — but it covers the most common tail-sampling use
/// case (capture error bursts, ignore healthy traffic).
#[derive(Debug)]
pub struct TailBasedSampler {
    /// Window size (number of recent observations).
    window_size: usize,
    /// Error rate threshold in `[0.0, 1.0]`; above this, record.
    error_threshold: f64,
    /// Sliding window of (was_error) outcomes, newest at the end.
    window: Mutex<Vec<bool>>,
    /// True when an error burst was detected and the next span should
    /// be recorded. Cleared after one record to avoid runaway recording.
    armed: Mutex<bool>,
}

impl TailBasedSampler {
    /// Construct a tail-based sampler with default window size (100) and
    /// default error threshold (0.10 = 10%).
    pub fn new() -> Self {
        Self::with_params(100, 0.10)
    }

    /// Construct a tail-based sampler with explicit window and threshold.
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

    /// Convenience constructor for tests: build with a fixed window and
    /// observe the supplied outcomes in order, then return a sampler
    /// whose `should_sample` reflects the current error rate.
    #[cfg(test)]
    pub fn from_outcomes(window_size: usize, outcomes: &[bool]) -> Self {
        let sampler = Self::with_params(window_size, 0.10);
        for &was_error in outcomes {
            sampler.observe(&SpanContext::root("t", "s", false), was_error);
        }
        sampler
    }

    /// True when the error rate in the current window strictly exceeds
    /// the threshold. Empty windows are considered 0% error rate.
    #[allow(dead_code)]
    fn error_rate_exceeds(&self) -> bool {
        let window = lock_or_recover(&self.window, "TailBasedSampler::error_rate_exceeds");
        if window.is_empty() {
            return false;
        }
        let errors = window.iter().filter(|e| **e).count();
        let rate = errors as f64 / window.len() as f64;
        rate > self.error_threshold
    }
}

impl Default for TailBasedSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler for TailBasedSampler {
    fn name(&self) -> &str {
        "tail-based"
    }

    fn should_sample(&self, _ctx: &SpanContext) -> SamplingDecision {
        // If a previous observation armed the sampler (error rate crossed
        // the threshold), record once then disarm. The single-shot behavior
        // matches what most production tail samplers do: don't capture
        // every span after the burst, just enough to characterize it.
        let mut armed = lock_or_recover(&self.armed, "TailBasedSampler::should_sample");
        if *armed {
            *armed = false;
            return SamplingDecision::Record;
        }
        SamplingDecision::Drop
    }

    fn observe(&self, _ctx: &SpanContext, was_error: bool) {
        // Push the outcome onto the sliding window; evict oldest if full.
        let mut window = lock_or_recover(&self.window, "TailBasedSampler::observe (window)");
        if window.len() >= self.window_size {
            window.remove(0);
        }
        window.push(was_error);

        // If the new error rate crosses the threshold, arm the sampler so
        // the next should_sample call records.
        let errors = window.iter().filter(|e| **e).count();
        let rate = errors as f64 / window.len().max(1) as f64;
        if rate > self.error_threshold {
            let mut armed = lock_or_recover(&self.armed, "TailBasedSampler::observe (armed)");
            *armed = true;
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn child_of(parent: SpanContext) -> SpanContext {
        SpanContext::root("child-trace", "child-span", false).with_parent(parent)
    }

    #[test]
    fn parent_based_honors_sampled_parent() {
        let sampler = ParentBasedSampler::new();
        // Parent sampled → child recorded (regardless of child flags).
        let parent = SpanContext::root("trace-1", "span-1", true);
        let child = child_of(parent);
        assert_eq!(sampler.should_sample(&child), SamplingDecision::Record);
    }

    #[test]
    fn parent_based_honors_unsampled_parent() {
        let sampler = ParentBasedSampler::new();
        // Parent not sampled → child dropped (regardless of child flags).
        let parent = SpanContext::root("trace-1", "span-1", false);
        let child = child_of(parent);
        assert_eq!(sampler.should_sample(&child), SamplingDecision::Drop);
    }

    #[test]
    fn parent_based_uses_self_flag_when_no_parent() {
        let sampler = ParentBasedSampler::new();
        let sampled = SpanContext::root("t", "s", true);
        let not_sampled = SpanContext::root("t", "s", false);
        assert_eq!(sampler.should_sample(&sampled), SamplingDecision::Record);
        assert_eq!(sampler.should_sample(&not_sampled), SamplingDecision::Drop);
    }

    #[test]
    fn parent_based_propagates_through_multiple_ancestors() {
        let sampler = ParentBasedSampler::new();
        // Three-level chain: root sampled → middle → leaf. Leaf must
        // inherit the upstream decision.
        let root = SpanContext::root("t", "r", true);
        let middle = child_of(root);
        let leaf = child_of(middle);
        assert_eq!(sampler.should_sample(&leaf), SamplingDecision::Record);
    }

    #[test]
    fn always_and_never_samplers_are_constant() {
        let always = AlwaysSampler;
        let never = NeverSampler;
        let ctx = SpanContext::root("t", "s", false);
        assert_eq!(always.should_sample(&ctx), SamplingDecision::Record);
        assert_eq!(never.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn rate_limit_caps_at_n_per_sec() {
        // 100/sec for 1s; consume 200 quickly → ~100 should record.
        let sampler = RateLimitSampler::new(100.0);
        let ctx = SpanContext::root("t", "s", false);

        let mut recorded = 0;
        for _ in 0..200 {
            if sampler.should_sample(&ctx) == SamplingDecision::Record {
                recorded += 1;
            }
        }
        // Bucket starts full at max_burst == 100.0, so first ~100 tokens
        // are consumed immediately; remainder drain as time elapses (a few
        // hundred microseconds for 200 calls → ~0 extra). Allow a small
        // margin so the test isn't flaky on slow CI.
        assert!(
            (90..=110).contains(&recorded),
            "expected ~100 records at 100/sec over 200 calls, got {recorded}"
        );
    }

    #[test]
    fn rate_limit_refills_after_idle() {
        // 10/sec; consume the burst (10 tokens) → drop, then wait 200ms
        // and consume again — bucket should have refilled ~2 tokens.
        let sampler = RateLimitSampler::new(10.0);
        let ctx = SpanContext::root("t", "s", false);

        // Drain initial burst.
        for _ in 0..20 {
            sampler.should_sample(&ctx);
        }
        // At this point bucket is empty.
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);

        // Sleep long enough for ≥1 token to refill (100ms at 10/sec).
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Record);
    }

    #[test]
    fn tail_based_records_when_error_rate_exceeds_threshold() {
        // 5 observations: 4 errors + 1 ok = 80% error rate, threshold 10%.
        let sampler = TailBasedSampler::from_outcomes(10, &[true, true, true, false, true]);
        let ctx = SpanContext::root("t", "s", false);
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Record);
    }

    #[test]
    fn tail_based_drops_when_error_rate_below_threshold() {
        // 5 observations: 1 error = 20%, threshold 50% → drop.
        let sampler = TailBasedSampler::with_params(10, 0.50);
        let ctx = SpanContext::root("t", "s", false);
        for was_error in [false, true, false, false, false] {
            sampler.observe(&ctx, was_error);
        }
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn tail_based_armed_flag_is_single_shot() {
        // After arming, exactly one Record is returned; subsequent calls
        // return Drop until another arming observation arrives.
        let sampler = TailBasedSampler::with_params(10, 0.50);
        let ctx = SpanContext::root("t", "s", false);
        // All errors → arm.
        for _ in 0..10 {
            sampler.observe(&ctx, true);
        }
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Record);
        // Subsequent calls before new observations drop.
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn sampler_trait_is_object_safe() {
        // Compile-time check: the trait can be used as `dyn Sampler`.
        // If a future refactor accidentally adds a generic method or a
        // `Self: Sized` bound, this test fails to compile.
        fn _accept_dyn(_s: &dyn Sampler) {}
        _accept_dyn(&ParentBasedSampler::new());
        _accept_dyn(&AlwaysSampler);
        _accept_dyn(&NeverSampler);
        _accept_dyn(&RateLimitSampler::new(10.0));
        _accept_dyn(&TailBasedSampler::new());
    }

    #[test]
    fn sampling_decision_is_record_helper() {
        assert!(SamplingDecision::Record.is_record());
        assert!(!SamplingDecision::Drop.is_record());
    }

    #[test]
    fn span_context_root_sets_sampled_bit() {
        let sampled = SpanContext::root("t", "s", true);
        assert_eq!(sampled.trace_flags, 0x01);
        let not_sampled = SpanContext::root("t", "s", false);
        assert_eq!(not_sampled.trace_flags, 0x00);
        assert!(not_sampled.parent.is_none());
    }

    #[test]
    fn span_context_with_parent_links_chain() {
        let parent = SpanContext::root("p-trace", "p-span", true);
        let child = SpanContext::root("c-trace", "c-span", false).with_parent(parent.clone());
        assert!(child.parent.is_some());
        // is_sampled recurses into parent.
        assert!(child.is_sampled());
        // Parent's parent is None.
        assert!(parent.parent.is_none());
    }

    // -------------------------------------------------------------------------
    // T50: sampling boundary / edge-case tests
    // -------------------------------------------------------------------------

    #[test]
    fn span_context_empty_string_ids_are_valid() {
        // The sampler layer does not validate trace_id / span_id format —
        // that is the exporter's responsibility. Empty strings must not panic
        // and must preserve the sampled-bit logic correctly.
        let ctx = SpanContext::root("", "", true);
        assert!(ctx.is_sampled());
        let ctx_unsampled = SpanContext::root("", "", false);
        assert!(!ctx_unsampled.is_sampled());
    }

    #[test]
    fn span_context_trace_flags_zero_is_not_sampled() {
        // `trace_flags == 0x00` → sampled bit clear.
        let ctx = SpanContext {
            trace_id: "t".into(),
            span_id: "s".into(),
            trace_flags: 0x00,
            parent: None,
        };
        assert!(!ctx.is_sampled());
    }

    #[test]
    fn span_context_trace_flags_high_bits_dont_affect_sampled() {
        // Only bit 0 is the sampled bit per W3C Trace Context §7.1.1.
        // A flags byte with only upper bits set must still read as not-sampled.
        let ctx = SpanContext {
            trace_id: "t".into(),
            span_id: "s".into(),
            trace_flags: 0xFE, // bits 1-7 set, bit 0 clear
            parent: None,
        };
        assert!(!ctx.is_sampled());

        let ctx_sampled = SpanContext {
            trace_id: "t".into(),
            span_id: "s".into(),
            trace_flags: 0xFF, // all bits set, including bit 0
            parent: None,
        };
        assert!(ctx_sampled.is_sampled());
    }

    #[test]
    fn span_context_no_parent_unsampled_is_false() {
        // Root span with no parent and sampled bit clear → `is_sampled` false.
        // Explicitly exercises the `None` branch in `is_sampled`.
        let ctx = SpanContext::root("t", "s", false);
        assert!(ctx.parent.is_none());
        assert!(!ctx.is_sampled());
    }

    #[test]
    fn always_sampler_name_and_default() {
        let s = AlwaysSampler;
        assert_eq!(s.name(), "always");
        let s2 = AlwaysSampler::default();
        let ctx = SpanContext::root("t", "s", false);
        assert_eq!(s2.should_sample(&ctx), SamplingDecision::Record);
    }

    #[test]
    fn never_sampler_name_and_default() {
        let s = NeverSampler;
        assert_eq!(s.name(), "never");
        let s2 = NeverSampler::default();
        let ctx = SpanContext::root("t", "s", true);
        assert_eq!(s2.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn parent_based_sampler_name() {
        assert_eq!(ParentBasedSampler::new().name(), "parent-based");
    }

    #[test]
    fn rate_limit_sampler_name() {
        assert_eq!(RateLimitSampler::new(1.0).name(), "rate-limit");
    }

    #[test]
    fn tail_based_sampler_name() {
        assert_eq!(TailBasedSampler::new().name(), "tail-based");
    }

    #[test]
    fn rate_limit_zero_rate_panics() {
        // `rate_per_sec <= 0` is a contract violation; the constructor must
        // panic loudly rather than silently producing a broken sampler.
        let result = std::panic::catch_unwind(|| RateLimitSampler::new(0.0));
        assert!(result.is_err(), "zero rate_per_sec must panic");
    }

    #[test]
    fn rate_limit_negative_rate_panics() {
        let result = std::panic::catch_unwind(|| RateLimitSampler::new(-5.0));
        assert!(result.is_err(), "negative rate_per_sec must panic");
    }

    #[test]
    fn tail_based_zero_window_panics() {
        // window_size == 0 is a contract violation.
        let result = std::panic::catch_unwind(|| TailBasedSampler::with_params(0, 0.10));
        assert!(result.is_err(), "zero window_size must panic");
    }

    #[test]
    fn tail_based_threshold_at_zero_arms_on_any_error() {
        // With error_threshold == 0.0, a single error drives rate > 0.0 →
        // sampler arms immediately.
        let sampler = TailBasedSampler::with_params(10, 0.0);
        let ctx = SpanContext::root("t", "s", false);
        sampler.observe(&ctx, true); // 1 error → rate = 1.0 > 0.0 → arm
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Record);
    }

    #[test]
    fn tail_based_threshold_at_one_never_arms() {
        // With error_threshold == 1.0, even 100% errors don't exceed the
        // threshold (rate > threshold, not >=), so the sampler never arms.
        let sampler = TailBasedSampler::with_params(5, 1.0);
        let ctx = SpanContext::root("t", "s", false);
        for _ in 0..5 {
            sampler.observe(&ctx, true); // all errors, but rate == 1.0 == threshold
        }
        // rate (1.0) is NOT > threshold (1.0), so armed remains false.
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn tail_based_empty_window_drops() {
        // A freshly constructed sampler with no observations has rate 0 →
        // should_sample returns Drop (armed is false from the start).
        let sampler = TailBasedSampler::new();
        let ctx = SpanContext::root("t", "s", false);
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
    }

    #[test]
    fn rate_limit_with_burst_one_records_exactly_one() {
        // `max_burst = 1.0` means the bucket holds exactly one token.
        // The first call records, subsequent immediate calls drop.
        let sampler = RateLimitSampler::with_burst(1_000_000.0, 1.0);
        let ctx = SpanContext::root("t", "s", false);
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Record);
        assert_eq!(sampler.should_sample(&ctx), SamplingDecision::Drop);
    }
}
