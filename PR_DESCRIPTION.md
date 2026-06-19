Refreshes the substrate-meta-bundle for `pheno-tracing` per ADR-019 and v8 plan T15 (item T15.10 of 22).

What's added: `AGENTS.md` (per-repo template), `llms.txt` (curated README + CHANGELOG + WORKLOG + spec), `WORKLOG.md` (v2.1 schema — 11 columns including new `device:` field per ADR-025/030), `CHANGELOG.md` (Keep-a-Changelog), `LICENSE-MIT` (standard MIT), `.github/workflows/ci.yml`.

What's verified: `pheno-ci-templates` runs the test matrix + coverage gate (80% for libs, 60% for federated, N/A for `pheno-wtrees`) + OTLP smoke test (where applicable per `ci.otlp_smoke_test`).

What this does NOT do: no source-code modifications, no breaking API changes, no version bump on the crate, no dep changes. Those are separate PRs.

## Spec source

- Spec JSON: `/tmp/t15-specs/pheno-tracing.json`
- T15 item: chore/pheno-flake-refresh-pheno-tracing-2026-06-18
- ADRs: ADR-019 (substrate governance), ADR-022 (config consolidation), ADR-023 (substrate placement), ADR-025 (v2.1 bump), ADR-030 (v2.1 device column), ADR-040 (coverage gate).

## Files

- `AGENTS.md` — per-repo template
- `llms.txt` — curated list of files + public API
- `WORKLOG.md` — v2.1 schema (7 cols + `device`)
- `CHANGELOG.md` — Keep-a-Changelog
- `LICENSE-MIT` — MIT license (Koosha Pari 2026)
- `.github/workflows/ci.yml` — from `pheno-ci-templates` (test + clippy + fmt + 80% coverage gate)
- **OTLP smoke test**: included in the new CI workflow (`otlp_smoke_test` job) — verifies pheno-otel/pheno-tracing wiring compiles and `OTEL_EXPORTER_OTLP_ENDPOINT` env var is honored.

## Test count

- current_count: 9
- target_count: match or exceed current (9); add ≥1 integration test if absent
- coverage_threshold_pct: 80

## Labels

`governance`, `T15`, `L5-#119`, `language:rust`

## Branch

`chore/pheno-flake-refresh-pheno-tracing-2026-06-18` → `main`
