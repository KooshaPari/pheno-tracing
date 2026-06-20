# Contributing to pheno-tracing

Thanks for your interest in `pheno-tracing` — the canonical port-driven
distributed tracing substrate for the pheno-* fleet (ADR-036).

This document covers how to file issues, propose changes, and submit pull
requests. For the broader pheno-* governance model, see
[`AGENTS.md`](./AGENTS.md) and `SPEC.md`.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Project layout](#project-layout)
3. [Development setup](#development-setup)
4. [Workflow](#workflow)
5. [Coding conventions](#coding-conventions)
6. [Testing requirements](#testing-requirements)
7. [Documentation requirements](#documentation-requirements)
8. [Commit & PR conventions](#commit--pr-conventions)
9. [Release process](#release-process)
10. [Reporting security issues](#reporting-security-issues)

## Code of Conduct

This project and everyone participating in it is governed by the
[Contributor Covenant Code of Conduct](./CODE_OF_CONDUCT.md).
By participating, you are expected to uphold this code. Report unacceptable
behaviour to **koosha@pari.io**.

## Project layout

```
pheno-tracing/
├── src/
│   ├── lib.rs          # public re-exports + crate-level docs
│   ├── port.rs         # TracePort trait + port types (the boundary)
│   ├── adapters.rs     # InMemoryAdapter, StdoutAdapter (concrete impls)
│   └── config.rs       # figment-based TracingConfig (env + TOML)
├── tests/              # integration tests (one file per adapter)
├── AGENTS.md           # the substrate constitution (read first)
├── SPEC.md             # behavioural spec (normative)
├── WORKLOG.md          # v2.1 worklog (every v8 DAG task → 1 row)
├── VERSION.toml        # single source of truth for crate version
├── CHANGELOG.md        # Keep-a-Changelog
├── llms.txt            # curated AI-onboarding index
├── justfile            # tier-0 hygiene recipes (`just build`, `just test`, ...)
├── deny.toml           # cargo-deny configuration
├── .editorconfig       # whitespace + indent rules
├── .gitattributes      # line-ending + linguist hints
└── .github/
    ├── CODEOWNERS                 # required reviewers (@kooshapari)
    ├── ISSUE_TEMPLATE/            # bug + feature templates
    ├── PULL_REQUEST_TEMPLATE.md   # PR checklist
    └── workflows/                 # ci / audit / deny / scorecard / release
```

## Development setup

### Prerequisites

- **Rust** — see MSRV in [`Cargo.toml`](./Cargo.toml) (`rust-version = "1.75"`)
- **just** — `cargo install just` (or `brew install just`)
- **cargo-audit** — `cargo install cargo-audit --locked`
- **cargo-deny** — `cargo install cargo-deny --locked`
- **cargo-llvm-cov** — `cargo install cargo-llvm-cov --locked` (for coverage)

### First build

```bash
git clone https://github.com/KooshaPari/pheno-tracing.git
cd pheno-tracing
just build
```

### Run the full tier-0 quality gate

```bash
just grade      # build + test + lint + fmt + audit + deny
just coverage   # 80% line coverage gate (ADR-023 Rule 3.1)
```

`just ci` runs the full matrix including coverage — the same gate enforced on
GitHub Actions.

## Workflow

1. **Open an issue first** for non-trivial changes. Use the bug / feature
   templates. For security issues, follow [`SECURITY.md`](./SECURITY.md).
2. **Branch from `main`** using one of these patterns (per `AGENTS.md`):
   - `<layer>/<slug>-<YYYY-MM-DD>` — e.g. `l0/port-error-context-2026-06-20`
   - `chore/<req-id>-<slug>-<date>` — e.g. `chore/orch-v12-s1-013-tier0-2026-06-20`
3. **Implement** following the coding conventions below.
4. **Verify locally**: `just ci` must pass before opening a PR.
5. **Open a PR** against `main` using the [PR template](./.github/PULL_REQUEST_TEMPLATE.md).
   Reference the v8 DAG task ID in the body (e.g. `Refs T15.10`).
6. **Update `WORKLOG.md`** — append one row per v8 DAG task ID (schema v2.1).
7. **Wait for review** — `CODEOWNERS` requires `@kooshapari` sign-off.

## Coding conventions

- **Style**: `cargo fmt --all` (CI-enforced).
- **Lints**: `cargo clippy --all-targets -- -D warnings` (CI-enforced).
- **API stability**: `pheno_tracing::*` is `semver-stable`; breaking changes
  require a major version bump and ADR.
- **No `unwrap()` in library code** — use `?` + `thiserror` / `Result`.
- **No `panic!` in library code** unless documented as unreachable.
- **Public APIs require doc comments** — every `pub` item gets at least one
  example or a `///` explanation.
- **Errors** are `thiserror`-typed (`TracingError`, etc.) — never raw `String`.
- **No new dependencies** without an ADR or PR justification (ADR-019 §3).

## Testing requirements

- **80% line coverage** is the lib/SDK gate (ADR-023 Rule 3.1).
- **One integration test file per adapter** (see `tests/`).
- **Tests must run in CI** with `--all-features` and `--no-default-features`.
- **OTLP smoke test** required for any change touching `src/port.rs` or
  `src/adapters.rs` — see `otlp_smoke` job in `.github/workflows/ci.yml`.
- **Doc-tests** must pass (`cargo test --doc`).

## Documentation requirements

Every PR must update one or more of:

- `CHANGELOG.md` (Keep-a-Changelog; `feat:`/`fix:`/`chore:` block under
  `## [Unreleased]`).
- `WORKLOG.md` (one row per v8 DAG task, schema v2.1).
- `SPEC.md` (if behaviour changes — add a normative clause).
- `AGENTS.md` (only if the constitution itself changes — rare).

Do **not** create ad-hoc `*.md` docs files — extend the canonical files above.
The only docs files this repo contains are the canonical ones listed in
[`AGENTS.md`](./AGENTS.md).

## Commit & PR conventions

### Commit messages — Conventional Commits

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `build`, `ci`.

Examples:

```
feat(port): add TraceContext::current_span_id() helper
fix(adapters): StdoutAdapter no longer panics on empty payload
chore(workflows): pin GitHub Actions to commit SHAs
docs(worklog): append T15.10 row (2026-06-20)
```

### PR title — Conventional Commits

Same format as commit subject. PR titles feed the changelog automatically
(via `release-please` in `.github/workflows/release.yml`).

### PR body

Use [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md).
Always include:

- `Refs Txx.yy` — the v8 DAG task ID this PR closes
- `ADR-xxxx` — any ADR this PR implements or amends
- A `## Verification` section listing the local commands you ran

## Release process

Releases are fully automated via `release-please`:

1. Conventional commits land on `main`.
2. `release-please` opens a release PR with version bump + `CHANGELOG.md`.
3. Merging the release PR tags + publishes to crates.io.

Manual release (only if automation is broken):

```bash
just preflight
git tag -s v0.x.y -m "v0.x.y"
git push origin v0.x.y
```

See `VERSION.toml` for the current version and `Cargo.toml` for the registry
metadata.

## Reporting security issues

**Do not open a public issue.** Follow [`SECURITY.md`](./SECURITY.md) —
use GitHub's [private security advisory][adv] flow or email
**security@pari.io** directly.

[adv]: https://github.com/KooshaPari/pheno-tracing/security/advisories/new

---

Happy tracing. The substrate is the contract.
