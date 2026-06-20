---
name: Bug report
about: Report a bug in pheno-tracing to help us improve
title: "[bug] "
labels: ["bug", "triage"]
assignees: []
---

## Bug Report

### Summary

A concise summary of the bug (one or two sentences).

### Environment

- **pheno-tracing version** (commit SHA or tag):
- **Rust toolchain** (`rustc --version`):
- **OS / arch**:
- **Feature flags enabled** (`--all-features`, `--no-default-features`, custom):
- **OTLP backend** (if applicable):

### Reproduction

Minimal, complete, verifiable reproduction:

```rust
// Minimal failing test (preferred)
#[tokio::test]
async fn repro() {
    // ...
}
```

Or, if a code snippet is not feasible, exact step-by-step reproduction:

1. `cargo add pheno-tracing`
2. `cargo new repro && cd repro`
3. Paste the following into `src/main.rs`:
   ```rust
   fn main() { /* ... */ }
   ```
4. `cargo run`
5. See error: …

### Expected behaviour

What you expected to happen.

### Actual behaviour

What actually happened (full output, stack traces, panic messages).

### Logs / traces

If relevant, attach:

- `RUST_LOG=trace cargo run …` output
- A `tracing-subscriber` JSON dump
- A reproducer that exercises `TracePort` end-to-end

### Workarounds

Any known workarounds (downgrade, skip a feature, alternate adapter, …).

### Acceptance criteria

What would a fix look like? Sketch a test or a sentence that the maintainer
could paste into the PR description.

### Checklist

- [ ] I have searched [existing issues](https://github.com/KooshaPari/pheno-tracing/issues) for duplicates
- [ ] I have read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`AGENTS.md`](./AGENTS.md)
- [ ] I am willing to [open a PR](https://github.com/KooshaPari/pheno-tracing/compare) with the fix
