# Hexagonal Ports — Adoption (pheno-tracing)

> **Adoption status:** ACTIVE (v17 Wave A T4, 2026-06-21)
> **Pattern reference:** [docs/architecture/hexagonal-ports.md](../../docs/architecture/hexagonal-ports.md)

This substrate **adopts and extends** the fleet-wide Hexagonal Port
pattern (ADR-038).

## Substrate role in the pattern

`pheno-tracing` is the canonical **distributed tracing** substrate
(ADR-036 / ADR-036B). It declares multiple port traits and ships
adapters for the common backends.

### Currently shipped ports

| Port | Module | Trait | Adapters |
| :-- | :-- | :-- | :-- |
| Trace submission | `port.rs` | `TracePort` | (in-tree adapters under `adapters.rs`) |
| Sampling policy | `sampling.rs` | `Sampler` / `HexSamplingPort` (alias) | `AlwaysSampler`, `NeverSampler`, `ParentBasedSampler`, `RateLimitSampler`, `TailBasedSampler` |
| Subscriber compat | `compat.rs` | `SubscriberAdapter` (+ `CollectorAdapter` extension) | (per-backend impls) |

## Local rules

- All adapters in this substrate MUST be `pub struct` types that
  implement at least one of the ports above (or a future `*Port`
  trait added in a v+1 cycle).
- The `HexSamplingPort` alias added in v12-04 is the canonical
  re-export name. New sampler backends MUST be `pub struct` types
  that `impl Sampler` (and therefore `HexSamplingPort`); do not
  re-introduce a parallel trait hierarchy.
- Cross-port dependencies (e.g. a trace backend that needs the
  current sampling decision) MUST be wired through the trait
  surface, not through direct concrete-type access.

## CI gate

```bash
# From the monorepo root:
./scripts/check-hex-ports.sh pheno-tracing
```

## See also

- [docs/architecture/hexagonal-ports.md](../../docs/architecture/hexagonal-ports.md) — canonical pattern
- [ADR-036B](https://github.com/KooshaPari/phenotype-monorepo/blob/main/docs/adr/2026-06-18/ADR-036B-pheno-tracing-substrate-canonical.md) — substrate canonical
- [ADR-038](https://github.com/KooshaPari/phenotype-monorepo/blob/main/docs/adr/2026-06-18/ADR-038-hexagonal-port-adapter-l4-policy.md) — policy
