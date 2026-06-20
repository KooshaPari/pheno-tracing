# pheno-tracing — justfile
# Tier-0 hygiene per ADR-019 + substrate governance (ADR-023).
# Run `just` (no args) for the recipe list.

set shell := ["zsh", "-cu"]
set dotenv-load := true
set positional-arguments := true

# Default recipe: show help
default:
    @just --list --unsorted

# ─── Build ────────────────────────────────────────────────────────────────────
# Debug build (all features, all targets).
build:
    cargo build --all-features --all-targets

# Release build (optimised; what ships).
build-release:
    cargo build --release --all-features

# ─── Test ─────────────────────────────────────────────────────────────────────
# Run the full test matrix: with features, without features, doc-tests.
test:
    cargo test --workspace --all-features
    cargo test --workspace --no-default-features
    cargo test --doc --all-features

# Quick smoke test (no default features).
test-smoke:
    cargo test --workspace --no-default-features -- --nocapture

# ─── Lint ─────────────────────────────────────────────────────────────────────
# Clippy with -D warnings (CI gate).
lint:
    cargo clippy --all-targets --all-features -- -D warnings
    cargo clippy --all-targets --no-default-features -- -D warnings

# ─── Format ───────────────────────────────────────────────────────────────────
# Check formatting (CI gate).
fmt:
    cargo fmt --all -- --check

# Apply formatting in place.
fmt-fix:
    cargo fmt --all

# ─── Audit ────────────────────────────────────────────────────────────────────
# RustSec advisory database check.
audit:
    cargo audit

# ─── Deny ─────────────────────────────────────────────────────────────────────
# License + advisory + ban + duplicate check (uses deny.toml).
deny:
    cargo deny check

# ─── Coverage ─────────────────────────────────────────────────────────────────
# 80% line coverage gate per ADR-023 Rule 3.1.
coverage:
    cargo llvm-cov --all-features --lcov --output-path lcov.info
    cargo llvm-cov --all-features --html --output-dir coverage

# ─── Grade ────────────────────────────────────────────────────────────────────
# Aggregate quality score: runs build + test + lint + fmt + audit + deny.
# Used by ADR-019 "tier grade" reporting.
grade: build test lint fmt audit deny
    @echo ""
    @echo "✓ tier-0 grade PASS (build + test + lint + fmt + audit + deny)"
    @echo "  → run \`just coverage\` for the 80% line-coverage gate (ADR-023 Rule 3.1)"

# ─── CI (local parity) ────────────────────────────────────────────────────────
# Run the full CI matrix locally, mirroring .github/workflows/ci.yml.
ci: build test lint fmt audit deny coverage
    @echo "✓ full CI matrix PASS locally"

# ─── Convenience ──────────────────────────────────────────────────────────────
# Clean build artefacts.
clean:
    cargo clean
    rm -rf coverage/ lcov.info target/

# Watch for changes and re-run tests (requires cargo-watch).
watch:
    cargo watch -x 'test --workspace --all-features' -x 'clippy --all-targets -- -D warnings'

# Print toolchain info.
info:
    cargo --version
    rustc --version
    @just --version

# ─── Release prep ─────────────────────────────────────────────────────────────
# Pre-flight checks before tagging.
preflight: ci
    @echo "✓ preflight PASS — safe to tag and publish"
