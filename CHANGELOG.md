# Changelog

All notable changes to `pheno-tracing` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- AGENTS.md (per-repo template, ADR-019)
- llms.txt (curated README + CHANGELOG + WORKLOG + spec)
- WORKLOG.md (v2.1 schema — 7 columns including new `device:` field per ADR-015/025/030)
- CHANGELOG.md (Keep-a-Changelog)
- LICENSE-MIT (standard MIT, copyright Koosha Pari 2026)
- `justfile` — tier-0 hygiene recipes (`build`, `build-release`, `test`, `test-smoke`, `lint`, `fmt`, `fmt-fix`, `audit`, `deny`, `coverage`, `grade`, `ci`, `clean`, `watch`, `info`, `preflight`)
- `deny.toml` — cargo-deny configuration (licenses, advisories, bans, sources)
- `.editorconfig` — utf-8, LF, 4-space indent, rust max_line_length 100
- `.gitattributes` — `text=auto eol=lf`, `Cargo.lock` marked linguist-generated, `target/` `coverage/` `export-ignore`
- `CODE_OF_CONDUCT.md` — Contributor Covenant v2.1
- `CONTRIBUTING.md` — full contributor guide (setup, workflow, conventions, release)
- `SECURITY.md` — coordinated disclosure policy + 72 h / 7 d / 14–90 d SLAs
- `.github/CODEOWNERS` — `@kooshapari` required on every surface area
- `.github/ISSUE_TEMPLATE/bug_report.md`
- `.github/ISSUE_TEMPLATE/feature_request.md`
- `.github/ISSUE_TEMPLATE/config.yml` (issue chooser + security.md link)
- `.github/PULL_REQUEST_TEMPLATE.md`
- `.github/workflows/audit.yml` — RustSec + cargo-deny + Dependabot metadata (weekly cron + per-PR)
- `.github/workflows/deny.yml` — cargo-deny (per-PR + per-push)
- `.github/workflows/scorecard.yml` — OpenSSF Scorecard SARIF (weekly + per-push to main)
- `.github/workflows/release.yml` — release-please + cargo publish OIDC

### Changed
- `.github/workflows/ci.yml` — actions pinned to commit SHAs; Swatinem `rust-cache` added; coverage uploads lcov artifact (no codecov dep); concurrency group `ci-<workflow>-<ref>` with `cancel-in-progress: true`; OTLP smoke job renamed `otlp_smoke`

### Security
- All GitHub Actions pinned to full commit SHAs (third-party action hardening)
- `permissions:` block tightened to least-privilege per job
- `id-token: write` only on `release.yml` (trusted publishing OIDC)
