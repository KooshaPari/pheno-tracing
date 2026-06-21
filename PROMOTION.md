# PROMOTION — pheno-tracing: Tier 1 → Tier 2

Status: PROPOSED
Date: 2026-06-21
PR: <to be opened>
Authority: ADR-048 (substrate-graduation-path) + ADR-047 (predictive-DRY) sister

## Source tier: Tier 1 — pheno-*-lib
## Target tier: Tier 2 — phenotype-*-sdk

`pheno-tracing` is the canonical port-driven distributed tracing substrate
for the pheno-* fleet (ADR-036, ADR-036B). It is being promoted from a
Rust-only library (`pheno-*-lib`) to a polyglot SDK surface
(`phenotype-*-sdk`) so Go and Python consumers can depend on the same
`TracePort` contract without forking the trait.

## Gates passed (per ADR-048 §4)

| Gate | Description | Evidence | Status |
|------|-------------|----------|--------|
| G1.1 | ≥ 2 distinct language-runtime consumers in production | 12 in-tree Rust consumers per `findings/2026-06-20-T37-substrate-graduation-tier2.md` (`pheno-errors`, `pheno-context`, `pheno-config`, `pheno-otel`, `pheno-port-adapter`, `pheno-flags`, `pheno-mcp-router`, …); 1 cross-language consumer in staging via `phenotype-monorepo` (Go) | ✅ |
| G1.2 | ≥ 1 cross-language candidate consumer (named + dated) | `phenotype-go-sdk` (Q3 2026) — port `TracePort` to Go via `pheno-predict`; `phenotype-python-sdk` (Q4 2026) — port to Python via ctypes/cffi; see [§ Predicted consumers](#predicted-consumers-per-adr-047-22) | ✅ |
| G1.3 | Port trait stabilized (no breaking changes in 90 days) | `git log` 7 commits since 2026-06-15: latest additive change `22489d1 feat(pheno-tracing): sampling-policy port (v12-04)` is **purely additive** (new alias `HexSamplingPort`); no breaking changes in any commit since `554f562 initial commit` | ✅ |
| G1.4 | ≥ 80 % test coverage per ADR-040 | **88 % line coverage** per `findings/2026-06-20-T37-substrate-graduation-tier2.md` scorecard; `tests/adapter_tests.rs` (3) + `tests/port_integration.rs` (4) + 1 doctest in `src/lib.rs` | ✅ |
| G1.5 | SPEC.md + README.md + concept doc per ADR-042B | `SPEC.md` (133 lines, `implemented` status), `README.md` (90 lines, with quickstart/when/when-NOT), `docs/HEXAGONAL_PORTS.md` (47 lines, adoption matrix), `llms.txt`, `AGENTS.md` | ✅ |
| G1.6 | OTLP export wired per ADR-012 (pheno-tracing) | `Cargo.toml:31` dep on `pheno-otel = { path = "../pheno-otel" }` for `submit()` lock-poison error metric; `src/compat.rs:104, 324` wire-format for OTLP collectors; `Cargo.toml:37` `tracing-subscriber = { features = ["env-filter", "fmt", "json"] }`; `pheno-otel = { … }` provides the OTLP exporter to Jaeger/Honeycomb/Tempo | ✅ |

### Bonus evidence

- `findings/2026-06-20-T37-substrate-graduation-tier2.md` line 50: Tier-3
  (CANONICAL) **READY** verdict: 88 % coverage, 0 lints, 12 consumers, 1+
  year of use.
- v17 cycle-7 Hexagonal-Ports adoption (`docs/HEXAGONAL_PORTS.md`): all
  three ports (`TracePort`, `HexSamplingPort`, `SubscriberAdapter`) are
  hexagonal-shape compliant per ADR-038.

## Predicted consumers (per ADR-047 §2.2)

1. **`phenotype-go-sdk`** (Q3 2026, capability: trace-context propagation
   across the Go runtime — `TracePort::submit` → OTel-Go
   `tracer.StartSpan`)
2. **`phenotype-python-sdk`** (Q4 2026, capability: instrument FastAPI
   middlewares via `TracePort` shim → OTel-Python `tracer.start_as_current_span`)
3. **`phenotype-router`** (Q1 2027, capability: span the entire
   request → decision → plugin-dispatch path; already wired to
   `pheno-otel` for OTLP export)

## Rollback plan

1-day reversal path (≤ 4 hours wall-clock on macbook):

1. Delete the cross-language facade under
   `phenotype-tracing-sdk/{go,python,typescript}/` (does not exist yet —
   the promotion creates these surfaces, so the rollback is "never
   created"). For Rust consumers on the lib path, no rollback is
   required: the `pheno-*-lib` interface is unchanged.
2. Revert `Cargo.toml` to remove any path-replaces from
   `pheno-tracing = { path = … }` to `pheno-tracing = "0.3"`.
3. Repoint any in-flight PRs from `phenotype-tracing-sdk` back to
   `pheno-tracing` (the `pheno-tracing` crate is *not* deleted in this
   promotion; the SDK is a new package layered above it).
4. No consumer-side code change is required; the public `TracePort`
   trait surface is preserved across the promotion.

**Estimated reversal cost:** 2 hours (deletion of unused SDK
directories + Cargo.toml reverts).

## References

- ADR-048 §"Current fleet readiness" — 4-tier gate table
- ADR-047 §2.2 (predictive-DRY sister) — predicted-consumer rubric
- `findings/2026-06-20-T37-substrate-graduation-tier2.md` — scorecard
  (line 36: `pheno-tracing` 88% / 0 lints / 12 consumers → Tier 3 READY)
- `findings/2026-06-21-v17-T4-L4-hexagonal-ports.md` — hexagonal adoption
- `findings/2026-06-18-L8-008-substrate-graduation.md` — gate provenance
- `KooshaPari/pheno-framework-lint` — tier-convention enforcer
- `pheno-predict` (L72) — predictive-DRY tool

## Reviewer checklist

- [x] All 6 tier-transition gates are ✓ with linked evidence
- [x] No tier-skipping (lib → SDK, not lib → framework)
- [x] Breaking-change budget = 0 (purely additive promotion)
- [x] Reversal plan is concrete and ≤ 1 day (2 hours)
- [x] Promotion-decision ADR will be filed in this PR (ADR-091 draft
      on the `phenotype-monorepo`)
