# Changelog

All notable changes to `pheno-tracing` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed
- `pheno-otel` dependency. The crate previously listed
  `pheno-otel = { path = "../pheno-otel" }` for a single call site
  (`pheno_otel::metrics::record_error` on `InMemoryAdapter::submit` lock-poison
  recovery, ADR-036B). The `pheno-otel` crate is no longer published in the
  pheno-* fleet, which broke `cargo build` for any downstream consumer
  pulling `pheno-tracing` via git. The metrics call has been stubbed with a
  structured `tracing::error!` event (target `pheno_tracing.metrics`,
  fields `metric` and `reason`) so operator visibility is preserved through
  the standard `tracing-subscriber` pipeline and the crate now builds
  standalone with no external `pheno-*` dependencies.

### Changed
- Added doc comments to all public struct fields and enum variants in
  `src/port.rs` and `src/adapters.rs` to satisfy `#![warn(missing_docs)]`
  under `cargo clippy --all-targets -- -D warnings`.
- Added `let _phantom: PhantomData<T> = PhantomData;` to the
  `_check_same<T>` helper in `tests/sampling_port.rs` so clippy's
  `extra_unused_type_parameters` lint no longer fires.

### Added
- `src/compat.rs` — forward-compatibility shim for `tracing 0.1` → `tracing 0.2`
  (SOTA-async-trait-migration §3 tracing research, this turn). Provides:
  - Macro re-exports: `info!`, `warn!`, `error!`, `debug!`, `trace!`, `span!`,
    `instrument` available at both `pheno_tracing::*` and
    `pheno_tracing::compat::*` import paths.
  - `SubscriberAdapter` trait — thin wrapper over `tracing::Subscriber`'s
    common method set (the trait shape expected to be preserved under
    `tracing::Collector` on 0.2).
  - `CollectorAdapter` trait — supertrait of `SubscriberAdapter` on 0.1 via
    blanket impl. Flips to primary trait when 0.2 ships.
  - `TracingBackend` / `TracingVersion` / `SubscriberKind` — runtime facade
    for downstream code that needs to branch on the active tracing version.
  - `tracing-0-2` Cargo feature — opt-in flag that gates
    `tests/tracing-0-2-compat.rs`. No-op today (the dep is still 0.1); the
    feature exists so downstream consumers can enable forward-compat CI
    checks without pulling pre-release deps.
- `tests/tracing-0-2-compat.rs` — 6 forward-compat tests gated by
  `#[cfg(feature = "tracing-0-2")]`. Verifies macro re-exports, version
  detection (`current_backend_kind()`), the `SubscriberAdapter` trait object
  dispatch, and the `CollectorAdapter` supertrait relationship.

## [0.3.0-pre.0] - 2026-06-19

### Added
- AGENTS.md (per-repo template, ADR-019)
- llms.txt (curated README + CHANGELOG + WORKLOG + spec)
- WORKLOG.md (v2.1 schema — 7 columns including new `device:` field per ADR-015/025/030)
- CHANGELOG.md (Keep-a-Changelog)
- LICENSE-MIT (standard MIT, copyright Koosha Pari 2026)
- `.github/workflows/ci.yml` (from `KooshaPari/pheno-ci-templates`; test + clippy + fmt + 80% coverage gate)
