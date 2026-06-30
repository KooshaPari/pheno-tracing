//! Criterion benchmarks for the sampler hot path (L6 audit fix).
//!
//! Benchmarks cover the three sampler implementations and the `SpanContext`
//! parent-chain traversal, which runs on every `should_sample` call.
//!
//! Run: `cargo bench`
//! Filter: `cargo bench --bench sampler_benchmarks -- rate_limit`
//!
//! On a modern x86_64 / Apple Silicon host, expect:
//! - `rate_limit_consume`: ~200-400 ns per call (single-threaded token check)
//! - `tail_based_observe`: ~500-800 ns per call (sliding window push + scan)
//! - `parent_based_deep_chain`: ~100-300 ns (recursive flag check in depth 16)
//! - `span_context_is_sampled`: ~100-200 ns (depth 32 hex comparison)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pheno_tracing::{
    AlwaysSampler, NeverSampler, ParentBasedSampler, RateLimitSampler, Sampler, SpanContext,
    TailBasedSampler,
};

// =============================================================================
// Benchmark helpers
// =============================================================================

/// Build a `SpanContext` chain of the given depth, where the root has
/// `sampled=true` and `depth-1` children inherit via `with_parent`.
fn chain_of_depth(depth: usize, root_sampled: bool) -> SpanContext {
    let root = SpanContext::root("root-trace", "root-span", root_sampled);
    (0..depth.saturating_sub(1)).fold(root, |parent, _| {
        SpanContext::root("child-trace", "child-span", false).with_parent(parent)
    })
}

// =============================================================================
// RateLimitSampler benchmarks
// =============================================================================

fn bench_rate_limit_consume(c: &mut Criterion) {
    let sampler = RateLimitSampler::new(100_000.0);
    let ctx = SpanContext::root("t", "s", false);

    c.bench_function("rate_limit_consume", |b| {
        b.iter(|| black_box(sampler.should_sample(&ctx)))
    });
}

fn bench_rate_limit_consume_exhausted(c: &mut Criterion) {
    let sampler = RateLimitSampler::with_burst(1.0, 1.0);
    let ctx = SpanContext::root("t", "s", false);
    sampler.should_sample(&ctx);

    c.bench_function("rate_limit_consume_exhausted", |b| {
        b.iter(|| black_box(sampler.should_sample(&ctx)))
    });
}

// =============================================================================
// TailBasedSampler benchmarks
// =============================================================================

fn bench_tail_based_observe(c: &mut Criterion) {
    let sampler = TailBasedSampler::with_params(500, 0.10);
    let ctx = SpanContext::root("t", "s", false);

    c.bench_function("tail_based_observe", |b| {
        b.iter(|| {
            sampler.observe(&ctx, true);
            black_box(sampler.should_sample(&ctx))
        })
    });
}

fn bench_tail_based_observe_window_full(c: &mut Criterion) {
    let sampler = TailBasedSampler::with_params(1000, 0.10);
    let ctx = SpanContext::root("t", "s", false);
    for _ in 0..1000 {
        sampler.observe(&ctx, false);
    }

    c.bench_function("tail_based_observe_window_full", |b| {
        b.iter(|| {
            sampler.observe(&ctx, true);
            black_box(sampler.should_sample(&ctx))
        })
    });
}

// =============================================================================
// ParentBasedSampler benchmarks
// =============================================================================

fn bench_parent_based_shallow(c: &mut Criterion) {
    let sampler = ParentBasedSampler::new();
    let ctx = chain_of_depth(2, true);

    c.bench_function("parent_based_shallow", |b| {
        b.iter(|| black_box(sampler.should_sample(&ctx)))
    });
}

fn bench_parent_based_deep_chain(c: &mut Criterion) {
    let sampler = ParentBasedSampler::new();
    let ctx = chain_of_depth(16, true);

    c.bench_function("parent_based_deep_chain", |b| {
        b.iter(|| black_box(sampler.should_sample(&ctx)))
    });
}

// =============================================================================
// SpanContext::is_sampled traversal benchmarks
// =============================================================================

fn bench_span_context_is_sampled_deep(c: &mut Criterion) {
    let ctx = chain_of_depth(32, true);

    c.bench_function("span_context_is_sampled_deep", |b| {
        b.iter(|| black_box(ctx.is_sampled()))
    });
}

fn bench_span_context_is_sampled_shallow(c: &mut Criterion) {
    let ctx = chain_of_depth(1, false);

    c.bench_function("span_context_is_sampled_shallow", |b| {
        b.iter(|| black_box(ctx.is_sampled()))
    });
}

// =============================================================================
// Trivial sampler overhead
// =============================================================================

fn bench_always_sampler(c: &mut Criterion) {
    let ctx = SpanContext::root("t", "s", false);
    c.bench_function("always_sampler", |b| {
        b.iter(|| black_box(AlwaysSampler.should_sample(&ctx)))
    });
}

fn bench_never_sampler(c: &mut Criterion) {
    let ctx = SpanContext::root("t", "s", false);
    c.bench_function("never_sampler", |b| {
        b.iter(|| black_box(NeverSampler.should_sample(&ctx)))
    });
}

// =============================================================================
// Criterion boilerplate
// =============================================================================

criterion_group!(
    name = rate_limit;
    config = Criterion::default().sample_size(100);
    targets =
        bench_rate_limit_consume,
        bench_rate_limit_consume_exhausted,
);

criterion_group!(
    name = tail_based;
    config = Criterion::default().sample_size(100);
    targets =
        bench_tail_based_observe,
        bench_tail_based_observe_window_full,
);

criterion_group!(
    name = parent_traversal;
    config = Criterion::default().sample_size(100);
    targets =
        bench_parent_based_shallow,
        bench_parent_based_deep_chain,
        bench_span_context_is_sampled_deep,
        bench_span_context_is_sampled_shallow,
);

criterion_group!(
    name = trivial;
    config = Criterion::default().sample_size(50);
    targets =
        bench_always_sampler,
        bench_never_sampler,
);

criterion_main!(rate_limit, tail_based, parent_traversal, trivial);
