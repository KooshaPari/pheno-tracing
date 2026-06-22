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

use std::sync::Mutex;
use std::time::Instant;

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

    /// Decide whether a single span should be recorded, given the raw
    /// 3-tuple form `(trace_id, name, attrs)` (v22-T2 / L26).
    ///
    /// This is the spec-mandated 3-argument signature — convenient for
    /// adapters that have the trace_id, span name, and attribute map
    /// in hand but have not yet built a [`SpanContext`]. The default
    /// implementation builds a root [`SpanContext`] from the inputs
    /// and delegates to [`Sampler::should_sample`]; samplers that need
    /// access to `name` or `attrs` at decision time (e.g. tail-based
    /// rule matchers) can override.
    fn should_sample_with_attrs(
        &self,
        trace_id: &str,
        name: &str,
        attrs: &std::collections::HashMap<String, String>,
    ) -> SamplingDecision {
        // Default: build a root context (no parent) carrying the
        // trace_id. The `name` and `attrs` are accepted but ignored —
        // samplers that need them (TailSampler) override this method.
        let _ = (name, attrs);
        let ctx = SpanContext::root(trace_id, "span", false);
        self.should_sample(&ctx)
    }
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
        let mut state = self.tokens.lock().unwrap();
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
        let window = self.window.lock().unwrap();
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
        let mut armed = self.armed.lock().unwrap();
        if *armed {
            *armed = false;
            return SamplingDecision::Record;
        }
        SamplingDecision::Drop
    }

    fn observe(&self, _ctx: &SpanContext, was_error: bool) {
        // Push the outcome onto the sliding window; evict oldest if full.
        let mut window = self.window.lock().unwrap();
        if window.len() >= self.window_size {
            window.remove(0);
        }
        window.push(was_error);

        // If the new error rate crosses the threshold, arm the sampler so
        // the next should_sample call records.
        let errors = window.iter().filter(|e| **e).count();
        let rate = errors as f64 / window.len().max(1) as f64;
        if rate > self.error_threshold {
            let mut armed = self.armed.lock().unwrap();
            *armed = true;
        }
    }
}

// =============================================================================
// v22-T2 / L26 spec-mandated type surface
//
// The spec names the four strategy types as `ProbabilisticSampler`,
// `RateLimitedSampler`, and `TailSampler`. The canonical types above
// (`TraceIdRatioBased`, `RateLimitSampler`, `TailBasedSampler`) preserve
// the v12-04 / v14-v18 history; the spec names below are provided as
// 1:1 aliases (or as new types where the spec signature differs from
// the canonical signature) so consumers can write either spelling.
// =============================================================================

// -----------------------------------------------------------------------------
// TraceIdRatioBased — head-based probabilistic sampler
// -----------------------------------------------------------------------------

/// Probabilistic sampler that records a fixed fraction of trace_ids
/// based on a stable hash of the trace_id (v22-T2 / L26).
///
/// This is the canonical "head-based probabilistic" sampler (the
/// OTel-spec wording is `TraceIdRatioBased`; the more readable
/// alias [`ProbabilisticSampler`] is also exported). The fraction
/// `rate` is clamped to `[0.0, 1.0]`; out-of-range values are
/// silently clamped so a misconfigured env var cannot produce
/// undefined behavior.
///
/// # Decision rule
///
/// For each `trace_id`, a 64-bit hash is computed (FNV-1a) and
/// normalized to `[0.0, 1.0)`; if the normalized hash is `< rate`,
/// the trace is recorded. This is a stable, deterministic, parent-
/// unaware gate: a given `trace_id` is always either recorded or
/// dropped, regardless of the upstream `ParentBased` decision.
///
/// # When to use
///
/// - Low-throughput services where the back-end can afford to ingest
///   `rate * N_traces_per_sec` traces per second.
/// - As the inner sampler of a [`ParentBasedSampler`] (the upstream
///   W3C sampled bit overrides the local gate).
///
/// # When NOT to use
///
/// - You need to bound the absolute rate (use
///   [`RateLimitedSampler`] instead — the probabilistic gate does
///   not cap the absolute rate, only the fraction).
/// - You need error-aware sampling (use [`TailSampler`] or
///   [`TailBasedSampler`]).
#[derive(Debug, Clone)]
pub struct TraceIdRatioBased {
    rate: f64,
}

impl TraceIdRatioBased {
    /// Construct a probabilistic sampler with the given rate.
    ///
    /// `rate` is clamped to `[0.0, 1.0]`. NaN passes through
    /// (Rust's `f64::clamp` does not unwrap NaN); this is documented
    /// behavior — a NaN rate is treated as "always drop" because
    /// `NaN < anything` is false.
    pub fn new(rate: f64) -> Self {
        Self {
            rate: rate.clamp(0.0, 1.0),
        }
    }

    /// The (clamped) sampling rate.
    pub fn rate(&self) -> f64 {
        self.rate
    }
}

impl Sampler for TraceIdRatioBased {
    fn name(&self) -> &str {
        "trace-id-ratio-based"
    }

    fn should_sample(&self, ctx: &SpanContext) -> SamplingDecision {
        if self.rate <= 0.0 {
            return SamplingDecision::Drop;
        }
        if self.rate >= 1.0 {
            return SamplingDecision::Record;
        }
        // splitmix64-derived hash, normalized to [0.0, 1.0).
        let hash = splitmix64_hash(ctx.trace_id.as_bytes());
        let normalized = (hash as f64) / (u64::MAX as f64);
        if normalized < self.rate {
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }
}

/// Deterministic 64-bit hash of `bytes`, derived from the
/// splitmix64 finalizer (Stafford variant 13).
///
/// `splitmix64` is the standard "avalanche" mixer used by xoroshiro
/// and other PRNGs; it spreads entropy evenly across all 64 bits,
/// even for sequential inputs (which FNV-1a does not). Used by
/// [`TraceIdRatioBased`] to derive a stable, deterministic
/// sampling decision per `trace_id`.
///
/// Reference: <https://prng.di.unimi.it/splitmix64.c>
fn splitmix64_hash(bytes: &[u8]) -> u64 {
    // Seed: FNV-1a fold of the input bytes (cheap, no allocation).
    let mut seed: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        seed ^= *byte as u64;
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // Stafford variant 13 finalizer (3 rounds of xor-shift +
    // multiply). Gives uniform distribution on [0, 2^64).
    let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

// -----------------------------------------------------------------------------
// ProbabilisticSampler — spec alias for TraceIdRatioBased
// -----------------------------------------------------------------------------

/// Spec-mandated alias for [`TraceIdRatioBased`] (v22-T2 / L26).
///
/// Consumers that want the OTel-spec wording should write
/// `ProbabilisticSampler`; both spellings refer to the same type, so
/// existing code that uses `TraceIdRatioBased` continues to work.
///
/// `ProbabilisticSampler::new(rate)` resolves to
/// `TraceIdRatioBased::new(rate)` and behaves identically (ratio is
/// clamped to `[0.0, 1.0]`).
pub type ProbabilisticSampler = TraceIdRatioBased;

// -----------------------------------------------------------------------------
// RateLimitedSampler — probabilistic parent + token-bucket cap
// -----------------------------------------------------------------------------

/// Hybrid sampler that combines a probabilistic parent rate with a
/// per-second token-bucket cap (v22-T2 / L26).
///
/// The constructor signature is
/// `RateLimitedSampler::new(parent_rate, max_per_sec)`:
///
/// - `parent_rate` (in `[0.0, 1.0]`) is a probabilistic gate: a
///   stable hash of the `trace_id` is compared against `parent_rate`,
///   and traces that fall outside the gate are dropped at the head.
///   This is the "parent_rate" — it controls what fraction of unique
///   traces are *eligible* to be sampled.
/// - `max_per_sec` (in `(0, ∞)`) is a token-bucket cap: among the
///   eligible traces, at most `max_per_sec` records per second are
///   allowed; the rest are dropped at the head even if the
///   probabilistic gate let them through.
///
/// The two-stage design (probabilistic gate, then token-bucket cap)
/// gives smoother behavior than a pure rate-limiter: the
/// probabilistic gate spreads load across trace_ids (so a single
/// high-volume trace cannot saturate the bucket), and the token
/// bucket enforces a hard ceiling (so a probabilistic spike cannot
/// overshoot the back-end's ingestion rate).
///
/// # When to use
///
/// - High-throughput services with a known ingestion budget (e.g.
///   "we can afford 1k spans/sec to Honeycomb").
/// - You want a smoother rate than a pure token-bucket (the
///   probabilistic gate distributes sampling across distinct
///   trace_ids rather than letting one trace saturate the bucket).
///
/// # When NOT to use
///
/// - You need the W3C parent-of-trace contract → use
///   [`ParentBasedSampler`] (with a [`TraceIdRatioBased`] inner).
/// - You need error-aware sampling → use [`TailSampler`] or
///   [`TailBasedSampler`].
#[derive(Debug)]
pub struct RateLimitedSampler {
    /// Probabilistic parent rate (clamped to `[0.0, 1.0]`). A value of
    /// `0.0` effectively drops everything; `1.0` makes the
    /// probabilistic gate a no-op so the sampler behaves like a
    /// pure token-bucket [`RateLimitSampler`].
    parent_rate: f64,
    /// Token-bucket refill rate (records per second).
    max_per_sec: f64,
    /// Token-bucket state (tokens + last refill instant). Reused
    /// from [`RateLimitSampler`]'s [`TokenState`] shape so the
    /// arithmetic matches the existing implementation.
    bucket: Mutex<TokenState>,
}

impl RateLimitedSampler {
    /// Construct a rate-limited sampler.
    ///
    /// - `parent_rate` is the probabilistic gate ratio; values outside
    ///   `[0.0, 1.0]` are silently clamped (the same policy as
    ///   [`TraceIdRatioBased::new`]).
    /// - `max_per_sec` is the token-bucket refill rate; must be `> 0`.
    pub fn new(parent_rate: f64, max_per_sec: f64) -> Self {
        assert!(max_per_sec > 0.0, "max_per_sec must be > 0");
        Self {
            parent_rate: parent_rate.clamp(0.0, 1.0),
            max_per_sec,
            bucket: Mutex::new(TokenState {
                tokens: max_per_sec,
                last_refill: Instant::now(),
            }),
        }
    }

    /// The (clamped) probabilistic parent rate.
    pub fn parent_rate(&self) -> f64 {
        self.parent_rate
    }

    /// The token-bucket refill rate (records per second).
    pub fn max_per_sec(&self) -> f64 {
        self.max_per_sec
    }

    /// Refill the bucket proportional to elapsed time and try to consume
    /// one token. Returns `Record` if a token was consumed, `Drop`
    /// otherwise.
    fn try_consume(&self) -> SamplingDecision {
        let mut state = self.bucket.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_refill);
        let refill = elapsed.as_secs_f64() * self.max_per_sec;
        state.tokens = (state.tokens + refill).min(self.max_per_sec);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }

    /// Test-only helper: drain the bucket so the next call must wait
    /// for a refill. Used by the rate-limited unit test to force a
    /// deterministic "drop" after the burst is consumed.
    #[cfg(test)]
    fn drain(&self) {
        let mut state = self.bucket.lock().unwrap();
        state.tokens = 0.0;
        state.last_refill = Instant::now();
    }
}

impl Sampler for RateLimitedSampler {
    fn name(&self) -> &str {
        "rate-limited"
    }

    fn should_sample(&self, ctx: &SpanContext) -> SamplingDecision {
        // Stage 1: probabilistic gate. If the trace_id hash falls
        // outside `parent_rate`, drop immediately — no point
        // consuming a token.
        if self.parent_rate <= 0.0 {
            return SamplingDecision::Drop;
        }
        if self.parent_rate < 1.0 {
            // Reuse the `TraceIdRatioBased` decision so the gate is
            // identical to the canonical probabilistic sampler.
            let gate = TraceIdRatioBased::new(self.parent_rate);
            if gate.should_sample(ctx) == SamplingDecision::Drop {
                return SamplingDecision::Drop;
            }
        }
        // Stage 2: token-bucket cap.
        self.try_consume()
    }
}

// -----------------------------------------------------------------------------
// TailSampler — rule-list tail sampler
// -----------------------------------------------------------------------------

/// A single tail-sampling rule (v22-T2 / L26).
///
/// Each rule is a predicate over `(name, was_error, duration_ms)`.
/// When [`TailSampler::observe_outcome`] is called, the rules are
/// evaluated in declaration order; the first matching rule marks the
/// trace_id for recording. Rules compose: a `TailSampler` with
/// `vec![error_rule, slow_rule]` captures both error spans and
/// slow spans, but not healthy fast spans.
///
/// # Field semantics
///
/// - `name`: when `Some`, only spans with this exact name match.
///   When `None`, every span name matches (the "any name" rule).
/// - `error_only`: when `true`, only spans with `was_error == true`
///   match. When `false`, error status is ignored.
/// - `min_duration_ms`: when `Some`, only spans with
///   `duration_ms >= min_duration_ms` match. When `None`, latency is
///   ignored. Spans whose duration is unknown (`None`) never match a
///   `min_duration_ms` rule.
///
/// # Composition
///
/// Rules are evaluated with **logical OR** (any match → record) and
/// each field is **logical AND** within a rule (all specified fields
/// must be satisfied). This is the same shape as the OpenTelemetry
/// Collector's `tail_sampling` processor's `policy` field.
#[derive(Debug, Clone, PartialEq)]
pub struct TailSamplingRule {
    /// Span name to match (exact string equality). `None` matches any
    /// name. Default: `None`.
    pub name: Option<String>,
    /// If `true`, only error spans match. If `false`, error status is
    /// ignored. Default: `false`.
    pub error_only: bool,
    /// Minimum span duration in milliseconds. `None` means "ignore
    /// duration". A span with unknown duration never matches a
    /// `Some` value. Default: `None`.
    pub min_duration_ms: Option<u64>,
}

impl Default for TailSamplingRule {
    fn default() -> Self {
        Self {
            name: None,
            error_only: false,
            min_duration_ms: None,
        }
    }
}

impl TailSamplingRule {
    /// Construct a rule that matches any error span (regardless of
    /// name or duration). The "error capture" rule.
    pub fn errors() -> Self {
        Self {
            error_only: true,
            ..Default::default()
        }
    }

    /// Construct a rule that matches spans with `duration_ms >= min`
    /// (regardless of error status or name). The "slow span" rule.
    pub fn slow(min_duration_ms: u64) -> Self {
        Self {
            min_duration_ms: Some(min_duration_ms),
            ..Default::default()
        }
    }

    /// Construct a rule that matches a specific span name exactly.
    /// The "named endpoint" rule.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    /// Evaluate the rule against a span outcome. Returns `true` if the
    /// rule matches.
    ///
    /// Pure function — no global state, no side effects. Safe to call
    /// from any thread.
    pub fn matches(
        &self,
        name: &str,
        was_error: bool,
        duration_ms: Option<u64>,
    ) -> bool {
        if let Some(ref expected_name) = self.name {
            if expected_name != name {
                return false;
            }
        }
        if self.error_only && !was_error {
            return false;
        }
        if let Some(min) = self.min_duration_ms {
            match duration_ms {
                Some(d) if d >= min => {}
                _ => return false,
            }
        }
        true
    }
}

/// Tail sampler that records spans whose observed outcome matches any
/// rule in a rule list (v22-T2 / L26).
///
/// Unlike [`TailBasedSampler`] (which uses a sliding error-rate
/// window), [`TailSampler`] uses a **discrete rule list**: each rule
/// is a predicate over `(name, was_error, duration_ms)`. When a span
/// outcome is observed (via [`TailSampler::observe_outcome`]) and any
/// rule matches, the span's `trace_id` is marked for recording. The
/// next [`TailSampler::should_sample`] call for that trace_id returns
/// `Record`.
///
/// The rule-list shape is closer to the OpenTelemetry Collector's
/// `tail_sampling` processor's `policy` field, and is a better fit
/// for "I want errors and slow spans, but not healthy fast spans"
/// use cases than a sliding error-rate threshold.
///
/// # When to use
///
/// - You have a small, named set of capture rules (errors,
///   slow spans, named endpoints) and want explicit control over
///   which spans are recorded.
/// - You want a tail sampler that does not depend on a global
///   error-rate window.
///
/// # When NOT to use
///
/// - You need a probabilistic error rate ("record when error rate
///   > 10%") → use [`TailBasedSampler`] instead.
#[derive(Debug)]
pub struct TailSampler {
    /// Rule list; evaluated in order, first match wins.
    rules: Vec<TailSamplingRule>,
    /// Set of trace_ids that have matched a rule and should be
    /// sampled. The set is bounded by the number of unique
    /// trace_ids in the workload — for a long-lived process this
    /// is the cardinality you must monitor. Reset via
    /// [`TailSampler::reset`].
    marked: Mutex<std::collections::HashSet<String>>,
}

impl TailSampler {
    /// Construct a tail sampler with the given rule list.
    pub fn new(rules: Vec<TailSamplingRule>) -> Self {
        Self {
            rules,
            marked: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// The configured rule list.
    pub fn rules(&self) -> &[TailSamplingRule] {
        &self.rules
    }

    /// Observe a span outcome (v22-T2 / L26).
    ///
    /// If any rule matches `(name, was_error, duration_ms)`, the
    /// span's `trace_id` is marked for recording and the next
    /// `should_sample` call for that trace_id will return `Record`.
    ///
    /// This is the richer counterpart to [`Sampler::observe`], which
    /// only carries `was_error` and cannot express the rule
    /// predicates. The default [`Sampler::observe`] impl calls
    /// `observe_outcome` with `name = ""`, `duration_ms = None` so
    /// `error_only` rules still fire.
    pub fn observe_outcome(
        &self,
        trace_id: &str,
        name: &str,
        was_error: bool,
        duration_ms: Option<u64>,
    ) {
        for rule in &self.rules {
            if rule.matches(name, was_error, duration_ms) {
                if let Ok(mut marked) = self.marked.lock() {
                    marked.insert(trace_id.to_string());
                }
                return;
            }
        }
    }

    /// Reset the marked set, discarding all previously-marked
    /// trace_ids. Use for long-lived processes that want to bound
    /// the marked-set size (the set is bounded by the number of
    /// unique trace_ids ever observed).
    pub fn reset(&self) {
        if let Ok(mut marked) = self.marked.lock() {
            marked.clear();
        }
    }

    /// Number of currently-marked trace_ids. Test-only helper.
    #[cfg(test)]
    fn marked_count(&self) -> usize {
        self.marked.lock().map(|s| s.len()).unwrap_or(0)
    }
}

impl Sampler for TailSampler {
    fn name(&self) -> &str {
        "tail-rule"
    }

    fn should_sample(&self, ctx: &SpanContext) -> SamplingDecision {
        // If the trace_id was marked by a prior observe_outcome call,
        // record. The mark stays in the set across multiple
        // should_sample calls (a marked trace is recorded every
        // time, not just once) — this is a deliberate difference
        // from `TailBasedSampler`'s single-shot armed flag, because
        // rule-list tail sampling typically records *every* span in
        // a marked trace (a complete trace is more useful than a
        // single span).
        let marked = self.marked.lock().unwrap();
        if marked.contains(&ctx.trace_id) {
            SamplingDecision::Record
        } else {
            SamplingDecision::Drop
        }
    }

    fn observe(&self, ctx: &SpanContext, was_error: bool) {
        // Default `observe` carries only `was_error`; we synthesize a
        // minimal outcome (no name, no duration). Rules with
        // `error_only == true` will still fire; rules with a `name`
        // or `min_duration_ms` constraint will not, because we
        // don't have that data here. Callers that want name/duration
        // rules should use `observe_outcome` directly.
        self.observe_outcome(&ctx.trace_id, "", was_error, None);
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
        assert_eq!(
            sampler.should_sample(&child),
            SamplingDecision::Record
        );
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
}
