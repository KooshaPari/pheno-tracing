---
name: Pull Request
about: Submit a change to pheno-tracing
title: ""
labels: []
assignees: []
---

## Summary

<!-- One or two sentences: what & why. -->

## Refs

<!-- Required: the v8 DAG task ID this PR closes. Format: Refs Txx.yy -->
Refs:

<!-- If this PR implements or amends an ADR, link it here. -->
ADR:

## Type of change

- [ ] Bug fix (`fix:`)
- [ ] New feature (`feat:`)
- [ ] Breaking change (`feat!:` or `fix!:`)
- [ ] Documentation (`docs:`)
- [ ] Refactor (`refactor:`)
- [ ] Test (`test:`)
- [ ] Build / CI (`build:` / `ci:`)
- [ ] Chore (`chore:`)

## What changed

<!-- Bulleted list of concrete changes (files, APIs, behaviours). -->

-

## Public API impact

<!-- Check one. -->
- [ ] No public API change
- [ ] Additive (new `pub` item, no breaking change)
- [ ] Breaking (call out every breaking change with migration note below)

<!-- If breaking, list the migration path. -->
Migration:

## Testing

<!-- Tick what you ran locally. Per CONTRIBUTING.md: `just ci` must pass. -->
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo test --workspace --no-default-features`
- [ ] `cargo test --doc --all-features`
- [ ] `cargo audit`
- [ ] `cargo deny check`
- [ ] `cargo llvm-cov --all-features` — coverage stays ≥ 80% (ADR-023 Rule 3.1)

## Documentation

<!-- Which canonical doc files you touched. -->
- [ ] `CHANGELOG.md` — `## [Unreleased]` block updated
- [ ] `WORKLOG.md` — row appended (schema v2.1; include `device:` column)
- [ ] `SPEC.md` — updated if behaviour changed
- [ ] `AGENTS.md` — updated only if the constitution itself changed
- [ ] `README.md` — updated if user-facing ergonomics changed
- [ ] `llms.txt` — updated if the curated AI index needs refreshing

## Verification

<!-- Paste or summarize the output of `just ci` (or the individual recipes).
     Per ADR-019 tier-0: evidence before assertions. -->

```text
$ just ci
... (paste output)
```

## Checklist

- [ ] I have read [`CONTRIBUTING.md`](./CONTRIBUTING.md), [`AGENTS.md`](./AGENTS.md), and [`SPEC.md`](./SPEC.md)
- [ ] My commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] I have NOT introduced new dependencies without an ADR or justification
- [ ] No `unwrap()`, `panic!()`, or `unsafe` was added to library code
- [ ] All new `pub` items have doc comments with at least one example
- [ ] Branch is up to date with `main` (`git fetch origin && git rebase origin/main`)
- [ ] `@kooshapari` is requested as a reviewer per `CODEOWNERS`
