//! Cardinality cap middleware for span attributes (v22-T2 / L26).
//!
//! Span attributes are the primary source of series cardinality in any
//! metrics/observability backend. A single high-cardinality attribute
//! (e.g. an unbounded `user_id` or `request_id` label) can blow up the
//! TSDB index, slow dashboards, and burn money. The fleet standard
//! (per the v22 / L26 acceptance criteria) is to cap cardinality at
//! **100 unique values per attribute name** — once the cap is hit,
//! additional distinct values are replaced with the overflow marker
//! `__other__`.
//!
//! # Middleware shape
//!
//! This module ships a [`CardinalityCap`] struct that holds the
//! cross-process state (a `HashMap<attribute_name, HashSet<seen_values>>`)
//! and a `process(&mut HashMap<String, String>) -> CardinalityReport`
//! method that applies the cap in-place to a single span's attribute
//! map. The struct is `Send + Sync` (it uses [`std::sync::Mutex`]
//! internally) and is designed to be wrapped in an `Arc<CardinalityCap>`
//! and shared across the call graph — the standard "config-driven
//! consumer" pattern that mirrors how
//! [`crate::sampling::Sampler`] trait objects are shared.
//!
//! ```text
//!   caller ── submit() ──▶ TracePort ──▶ CardinalityCap::process()
//!                                                │
//!                                                ▼
//!                                          (in-place cap)
//!                                                │
//!                                                ▼
//!                                            OTLP exporter
//! ```
//!
//! # Cap policy
//!
//! For each attribute name, the cap is **first-N-wins**:
//!
//! 1. The first `cap` distinct values observed for a given name are
//!    passed through verbatim.
//! 2. Once the cap is hit, additional distinct values are replaced
//!    with the overflow marker (default: `__other__`).
//! 3. Values that have already been seen (i.e. are in the
//!    `HashSet<seen_values>`) are passed through verbatim, even after
//!    the cap is hit, so the first `cap` values are always recorded
//!    exactly.
//!
//! The "first-N-wins" rule matches the OpenTelemetry Collector's
//! `attributes/limit` processor and the Prometheus
//! `sample_limit`-style cardinality limiters.
//!
//! # Hash-bucket memory bound
//!
//! The seen-set is a `HashMap<String, HashSet<String>>` (one bucket
//! per attribute name). Each bucket is independently bounded by
//! `cap`; once a bucket is full, additional distinct values are
//! discarded (replaced with the overflow marker) and the bucket
//! size stays at `cap`. The total memory used by the cap is therefore
//! bounded by `O(num_attribute_names * cap)` — for the fleet
//! default of `cap = 100` and a typical service with 50 distinct
//! attribute names, the cap uses ~50 KB of stable state.
//!
//! This is the "hash-bucket implementation that bounds memory" the
//! spec calls for. The bound is per-process; a fleet-wide cap
//! requires either a coordinator service (out of scope) or a
//! consistent-hash redistribution of the per-process cap across
//! instances (a future L26 sub-task).
//!
//! # When to use
//!
//! - Production services that emit OTLP spans with user-controlled
//!   attribute values (request IDs, user IDs, URLs, etc.).
//! - Fleet-wide enforcement of cardinality budgets — wrap the OTLP
//!   exporter in a `CardinalityCap::process` call and the
//!   back-end-side TSDB stays bounded.
//!
//! # When NOT to use
//!
//! - The cap would hide a real signal (e.g. you need every distinct
//!   `error_message`). Either bump the cap, or use a different
//!   attribute for the high-cardinality data.
//! - Pre-prod / debug builds where the cap would obscure the
//!   behavior you're trying to diagnose.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Default cap value, per the v22 / L26 spec: 100 unique values per
/// attribute name.
pub const DEFAULT_CAP: usize = 100;

/// Default overflow marker — the sentinel value substituted for
/// attribute values that exceed the cardinality cap.
pub const DEFAULT_OVERFLOW_MARKER: &str = "__other__";

// =============================================================================
// CardinalityReport
// =============================================================================

/// Report returned by [`CardinalityCap::process`].
///
/// Tells the caller (and the test suite) exactly what the cap did:
/// how many attributes were inspected, how many were kept verbatim,
/// and how many were replaced with the overflow marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CardinalityReport {
    /// Number of attributes inspected in the call (one per
    /// `HashMap` entry).
    pub inspected: usize,
    /// Number of attributes that were passed through verbatim
    /// (either below the cap, or already in the seen-set).
    pub kept: usize,
    /// Number of attributes that were replaced with the overflow
    /// marker.
    pub overflowed: usize,
}

// =============================================================================
// CardinalityCap
// =============================================================================

/// Cardinality cap middleware (v22-T2 / L26).
///
/// Holds the per-process "seen values" registry, applies the cap
/// in-place to a span's attribute map, and returns a
/// [`CardityReport`] describing what changed.
///
/// The constructor signature is
/// `CardinalityCap::new(max_unique_per_attr)`:
///
/// - `max_unique_per_attr` is the per-attribute-name cap (the
///   maximum number of distinct values to keep verbatim before
///   substituting the overflow marker).
///
/// # Thread safety
///
/// The struct holds the seen-set behind a [`std::sync::Mutex`], so it
/// is `Send + Sync` and can be shared across threads via `Arc`. The
/// mutex is held only for the duration of the `process` call, and the
/// critical section is O(n) in the number of attribute names — fine
/// for the per-span hot path.
///
/// # Hash-bucket memory bound
///
/// The seen-set is a `HashMap<name, HashSet<value>>`. Each bucket
/// is bounded by `max_unique_per_attr`; once full, additional
/// distinct values are discarded (replaced with the overflow marker)
/// and the bucket size stays at the cap. Total memory is therefore
/// `O(num_attribute_names * max_unique_per_attr)`. See the
/// module-level docs for the arithmetic.
#[derive(Debug)]
pub struct CardinalityCap {
    /// Maximum number of unique values per attribute name.
    cap: usize,
    /// Overflow marker substituted for values beyond the cap.
    overflow_marker: String,
    /// Per-attribute registry of values already seen. Backed by a
    /// `Mutex` for thread safety; the lock is held only for the
    /// duration of `process`.
    seen: Mutex<HashMap<String, HashSet<String>>>,
}

impl CardinalityCap {
    /// Construct a cardinality cap with a custom cap and the default
    /// overflow marker (`__other__`).
    ///
    /// `cap == 0` is allowed and means "replace every distinct value
    /// with the overflow marker" — useful for testing the cap path
    /// in isolation.
    pub fn new(max_unique_per_attr: usize) -> Self {
        Self::with_overflow_marker(max_unique_per_attr, DEFAULT_OVERFLOW_MARKER.to_string())
    }

    /// Construct with the v22 / L26 default cap (100) and the default
    /// overflow marker (`__other__`).
    pub fn with_default() -> Self {
        Self::new(DEFAULT_CAP)
    }

    /// Construct with a custom cap and a custom overflow marker.
    pub fn with_overflow_marker(max_unique_per_attr: usize, overflow_marker: String) -> Self {
        Self {
            cap: max_unique_per_attr,
            overflow_marker,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// The configured cap (max unique values per attribute name).
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// The configured overflow marker.
    pub fn overflow_marker(&self) -> &str {
        &self.overflow_marker
    }

    /// Apply the cardinality cap in-place to a single span's attribute
    /// map. Returns a [`CardinalityReport`] describing what changed.
    ///
    /// # Algorithm
    ///
    /// For each `(name, value)` entry in `attrs`:
    ///
    /// 1. Look up `name` in the seen registry. If absent, insert
    ///    `name → {value}` and pass `value` through verbatim.
    /// 2. If `name` is present, check if `value` is in the seen-set.
    ///    - If yes: pass `value` through verbatim (idempotent).
    ///    - If no and the seen-set is below the cap: insert `value`
    ///      into the seen-set and pass `value` through verbatim.
    ///    - If no and the seen-set is at or above the cap: replace
    ///      `value` in `attrs` with the overflow marker.
    ///
    /// The `attrs` map is mutated in-place; the same map can be
    /// re-used across calls (e.g. for a batch of spans).
    pub fn process(&self, attrs: &mut HashMap<String, String>) -> CardinalityReport {
        let mut report = CardinalityReport::default();
        let mut seen = match self.seen.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        for (name, value) in attrs.iter_mut() {
            report.inspected += 1;
            let entry = seen.entry(name.clone()).or_default();

            if entry.contains(value) {
                // Already seen this value for this attribute name;
                // pass through verbatim. The seen-set stays at the
                // same size.
                report.kept += 1;
                continue;
            }

            if entry.len() < self.cap {
                // Below the cap — record the new value and pass
                // through verbatim.
                entry.insert(value.clone());
                report.kept += 1;
            } else {
                // At or above the cap — replace with the overflow
                // marker. The seen-set is NOT updated with the new
                // value, so the first N values are always the
                // "winners" (the first-N-wins policy).
                *value = self.overflow_marker.clone();
                report.overflowed += 1;
            }
        }

        report
    }

    /// Reset the seen registry, discarding all previously-observed
    /// values. Useful for tests and for periodic resets in long-lived
    /// processes (e.g. a daily cron).
    pub fn reset(&self) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.clear();
        }
    }

    /// Number of distinct attribute names currently in the seen
    /// registry. Test-only helper.
    #[cfg(test)]
    fn seen_name_count(&self) -> usize {
        self.seen.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// Number of distinct values currently recorded for a given
    /// attribute name. Test-only helper.
    #[cfg(test)]
    fn seen_value_count(&self, name: &str) -> usize {
        self.seen
            .lock()
            .ok()
            .and_then(|s| s.get(name).map(|v| v.len()))
            .unwrap_or(0)
    }
}

impl Default for CardinalityCap {
    /// `CardinalityCap::default()` is the v22 / L26 spec default:
    /// cap = 100, overflow marker = `__other__`.
    fn default() -> Self {
        Self::with_default()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Test 1 — values below the cap pass through verbatim.
    ///
    /// For a fresh `CardinalityCap` with the default cap of 100,
    /// submitting 50 distinct values for the same attribute must
    /// leave all 50 unchanged, and the report must show 50
    /// `kept` / 0 `overflowed`.
    ///
    /// Note: each `process` call operates on a one-entry `HashMap`
    /// (a multi-entry HashMap can only carry one value per
    /// attribute name). The "report" counters accumulate across
    /// the 50 calls — the spec mandates `inspected` count entries
    /// in the input map, which is 1 per call here.
    #[test]
    fn cardinality_cap_keeps_values_under_threshold() {
        let cap = CardinalityCap::with_default();
        assert_eq!(cap.cap(), DEFAULT_CAP);
        assert_eq!(cap.overflow_marker(), DEFAULT_OVERFLOW_MARKER);

        // Submit 50 distinct values for `user_id` across 50
        // `process` calls. All 50 should be kept (cap=100).
        let mut total_inspected = 0;
        let mut total_kept = 0;
        let mut total_overflowed = 0;
        for i in 0..50 {
            let mut attrs = HashMap::new();
            attrs.insert("user_id".to_string(), format!("user-{i:03}"));
            let report = cap.process(&mut attrs);
            total_inspected += report.inspected;
            total_kept += report.kept;
            total_overflowed += report.overflowed;
            // The value must still be the original (not the overflow marker).
            assert_eq!(attrs.get("user_id").unwrap(), &format!("user-{i:03}"));
        }
        assert_eq!(total_inspected, 50);
        assert_eq!(total_kept, 50);
        assert_eq!(total_overflowed, 0);

        // The seen-set for `user_id` must have all 50 entries.
        assert_eq!(cap.seen_value_count("user_id"), 50);
    }

    /// Test 2 — values past the cap are replaced with the overflow
    /// marker (first-N-wins eviction).
    ///
    /// With a small cap (5), submitting 12 distinct values across
    /// 12 calls must keep the first 5 verbatim and replace the
    /// remaining 7 with the overflow marker. The aggregate report
    /// must show 5 kept, 7 overflowed. The seen-set must stay at
    /// exactly 5 entries (the first-N-wins cap is enforced).
    #[test]
    fn cardinality_cap_replaces_overflow_with_marker() {
        let cap = CardinalityCap::new(5);

        let mut total_inspected = 0;
        let mut total_kept = 0;
        let mut total_overflowed = 0;
        for i in 0..12 {
            let mut attrs = HashMap::new();
            attrs.insert("request_id".to_string(), format!("req-{i:03}"));
            let report = cap.process(&mut attrs);
            total_inspected += report.inspected;
            total_kept += report.kept;
            total_overflowed += report.overflowed;

            if i < 5 {
                // First 5 must be kept verbatim.
                assert_eq!(attrs.get("request_id").unwrap(), &format!("req-{i:03}"));
                assert_eq!(report.kept, 1);
                assert_eq!(report.overflowed, 0);
            } else {
                // Subsequent values must overflow.
                assert_eq!(attrs.get("request_id").unwrap(), DEFAULT_OVERFLOW_MARKER);
                assert_eq!(report.kept, 0);
                assert_eq!(report.overflowed, 1);
            }
        }
        assert_eq!(total_inspected, 12);
        assert_eq!(total_kept, 5, "first 5 distinct values must be kept");
        assert_eq!(total_overflowed, 7, "remaining 7 must be replaced");

        // The seen-set for `request_id` must have exactly 5 entries
        // (the first 5 values).
        assert_eq!(cap.seen_value_count("request_id"), 5);
    }

    /// Test 3 — repeated values are passed through verbatim even
    /// after the cap is hit.
    ///
    /// Submits a sequence of spans in three batches: the first 5
    /// values establish the cap, the next 7 overflow, and the final
    /// 3 are repeats of the first 3 — which must pass through
    /// verbatim (not be replaced) because they're already in the
    /// seen-set. This proves the "idempotent" path: a value in the
    /// seen-set is always passed through, regardless of the cap.
    ///
    /// Each `process` call operates on a one-entry `HashMap` (the
    /// HashMap API can only carry one value per attribute name).
    /// We loop over the values within each batch.
    #[test]
    fn cardinality_cap_preserves_already_seen_values() {
        let cap = CardinalityCap::new(5);

        // First batch: establish the cap with 5 distinct values.
        for v in ["a", "b", "c", "d", "e"] {
            let mut attrs = HashMap::new();
            attrs.insert("endpoint".to_string(), v.to_string());
            let r = cap.process(&mut attrs);
            assert_eq!(r.kept, 1);
            assert_eq!(r.overflowed, 0);
            assert_eq!(attrs.get("endpoint").unwrap(), &v.to_string());
        }
        assert_eq!(cap.seen_value_count("endpoint"), 5);

        // Second batch: 7 NEW values. All 7 should overflow.
        for v in ["f", "g", "h", "i", "j", "k", "l"] {
            let mut attrs = HashMap::new();
            attrs.insert("endpoint".to_string(), v.to_string());
            let r = cap.process(&mut attrs);
            assert_eq!(r.kept, 0);
            assert_eq!(r.overflowed, 1);
            assert_eq!(attrs.get("endpoint").unwrap(), DEFAULT_OVERFLOW_MARKER);
        }
        assert_eq!(cap.seen_value_count("endpoint"), 5, "seen-set must NOT grow past cap");

        // Third batch: 3 of the original 5 values + 2 NEW values.
        // The 3 originals must pass through verbatim; the 2 new
        // values must overflow.
        for (v, expect_kept) in [("a", true), ("b", true), ("c", true), ("m", false), ("n", false)]
        {
            let mut attrs = HashMap::new();
            attrs.insert("endpoint".to_string(), v.to_string());
            let r = cap.process(&mut attrs);
            assert_eq!(
                r.kept,
                if expect_kept { 1 } else { 0 },
                "{v}: already-seen values must pass through verbatim"
            );
            assert_eq!(
                r.overflowed,
                if expect_kept { 0 } else { 1 },
                "{v}: new values must overflow"
            );
            if expect_kept {
                assert_eq!(attrs.get("endpoint").unwrap(), &v.to_string());
            } else {
                assert_eq!(attrs.get("endpoint").unwrap(), DEFAULT_OVERFLOW_MARKER);
            }
        }
        assert_eq!(cap.seen_value_count("endpoint"), 5);
    }

    /// Test 4 — independent attribute names have independent
    /// seen-sets (i.e. the cap is per-name, not global).
    #[test]
    fn cardinality_cap_is_per_attribute_name() {
        let cap = CardinalityCap::new(2);
        // 3 distinct values for `name_a` → 2 kept, 1 overflowed.
        for (i, expect_kept) in [(0, true), (1, true), (2, false)] {
            let mut attrs = HashMap::new();
            attrs.insert("name_a".to_string(), format!("a{i}"));
            let r = cap.process(&mut attrs);
            assert_eq!(
                r.kept,
                if expect_kept { 1 } else { 0 },
                "name_a[{i}]: expected kept={expect_kept}"
            );
            assert_eq!(
                r.overflowed,
                if expect_kept { 0 } else { 1 },
                "name_a[{i}]: expected overflowed={}",
                !expect_kept
            );
        }

        // 3 distinct values for `name_b` (a DIFFERENT name) → 2
        // kept, 1 overflowed, because the cap is per-name.
        for (i, expect_kept) in [(0, true), (1, true), (2, false)] {
            let mut attrs = HashMap::new();
            attrs.insert("name_b".to_string(), format!("b{i}"));
            let r = cap.process(&mut attrs);
            assert_eq!(
                r.kept,
                if expect_kept { 1 } else { 0 },
                "name_b[{i}]: expected kept={expect_kept}"
            );
            assert_eq!(
                r.overflowed,
                if expect_kept { 0 } else { 1 },
                "name_b[{i}]: expected overflowed={}",
                !expect_kept
            );
        }
        assert_eq!(cap.seen_name_count(), 2);
    }

    /// Bonus test — `reset()` clears the seen registry, freeing
    /// the first-N-wins "winners" to be re-evaluated. Documents the
    /// intended use of `reset` for long-lived processes.
    #[test]
    fn cardinality_cap_reset_clears_seen_registry() {
        let cap = CardinalityCap::new(2);
        // Fill the cap for `k` with 2 winners, then overflow 3 more.
        for (i, expect_kept) in [(0, true), (1, true), (2, false), (3, false), (4, false)] {
            let mut attrs = HashMap::new();
            attrs.insert("k".to_string(), format!("v{i}"));
            let r = cap.process(&mut attrs);
            assert_eq!(r.kept, if expect_kept { 1 } else { 0 });
            assert_eq!(r.overflowed, if expect_kept { 0 } else { 1 });
        }
        assert_eq!(cap.seen_value_count("k"), 2);

        // After reset, the same 5 values should be re-evaluated: 2
        // new "winners", 3 overflowed.
        cap.reset();
        for (i, expect_kept) in [(0, true), (1, true), (2, false), (3, false), (4, false)] {
            let mut attrs = HashMap::new();
            attrs.insert("k".to_string(), format!("v{i}"));
            let r = cap.process(&mut attrs);
            assert_eq!(r.kept, if expect_kept { 1 } else { 0 });
            assert_eq!(r.overflowed, if expect_kept { 0 } else { 1 });
        }
    }
}
