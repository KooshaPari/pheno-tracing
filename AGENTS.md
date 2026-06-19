# pheno-tracing — AGENTS.md (Agent Constitution)

**Date:** 2026-06-18
**Status:** ACTIVE
**Substrate:** `pheno-*-lib` (ADR-023)
**MSRV:** see `Cargo.toml`

## Purpose

Canonical tracing init across all pheno-* repos (ADR-012). One-liner `init()` installs the tracing-subscriber (env-filter + JSON formatter); every fleet crate depends on this for consistent log output and OTLP export.

## Public API

```rust
pheno_tracing::init(service_name: &str) -> Result<(), TracingError>
pheno_tracing::init_with_format(service_name, Format) -> Result<(), TracingError>
pheno_tracing::Format (Plain, Json)
pheno_tracing::TracingError (SubscriberInit, EnvParse)
```

## Build & Test

```bash
cargo build --release
cargo test --workspace --all-features
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Conventions

- Commits: Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `build:`, `ci:`)
- Branch: `<layer>/<slug>-<YYYY-MM-DD>` or `chore/<req-id>-<slug>-<date>`
- WORKLOG: append 1 row to `WORKLOG.md` per v8 DAG task ID (schema v2.1, 7 cols + `device`)
- PRs: reference task ID in body, e.g. `Refs T15.<n>` (per the T15 v8 plan tracking)
- **Substrate placement** (ADR-023): this is a `pheno-*-lib` — pure reusable Rust library, single concern, single crate.
- **Test coverage gate**: 80% line coverage (ADR-023 Rule 3.1, lib/SDK gate).
- **Quality bar**: spec, README, test matrix, OTLP observability via pheno-tracing (ADR-012), 80% coverage, CI gate.

## Do-Not-Touch Zones

- `<archive>/` (stale work, archived intentionally)
- `<vendor>/`, `<node_modules>/` (third-party)
- `**/.git`, `**/Cargo.lock` (unless explicitly updating deps)
- files marked `# DO NOT EDIT` header

## Authority

- Spec JSON: `/tmp/t15-specs/pheno-tracing.json`
- Substrate governance: `docs/adr/2026-06-15/ADR-019-substrate-governance.md`
- WORKLOG schema: `pheno-worklog-schema` v2.1 (ADR-015 + ADR-025 + ADR-030)
- llms.txt: see `pheno-llms-txt`
