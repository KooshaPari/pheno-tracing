---
name: Feature request
about: Suggest a new feature for pheno-tracing
title: "[feat] "
labels: ["enhancement", "triage"]
assignees: []
---

## Feature Request

### Problem

A clear and concise description of what problem this feature would solve.
Ex. *I'm always frustrated when…* / *It is hard to do X today because…*

### Proposed solution

A clear and concise description of what you want to happen. Sketch the
public-API change in pseudo-Rust:

```rust
// Sketch — does NOT have to compile.
pub trait TracePort {
    // new method(s) you'd add
}
```

### Alternatives considered

What other approaches have you considered? Why is this one better?

- Alternative A — …
- Alternative B — …
- Status quo — …

### Use case

A concrete scenario where this feature would be used:

- Which adapter? (`InMemoryAdapter`, `StdoutAdapter`, future `OtlpAdapter`)
- Which consumer? (which pheno-* crate or external user)
- What does the **before** / **after** look like?

### Scope & impact

- **Public API change?** (breaking / additive / internal)
- **Documentation change** (SPEC.md, AGENTS.md, README.md)?
- **New dependencies?** (link the crate + license)
- **Test strategy** — what new tests would prove this works?
- **Coverage impact** — does this risk dropping us below the 80% gate
  (ADR-023 Rule 3.1)?

### ADR

If this change is architectural, link or propose an ADR (see
[`docs/adr/`](./docs/adr/) pattern in `AGENTS.md`).

### Checklist

- [ ] I have searched [existing issues](https://github.com/KooshaPari/pheno-tracing/issues) for duplicates
- [ ] I have read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`SPEC.md`](./SPEC.md)
- [ ] I am willing to [open a PR](https://github.com/KooshaPari/pheno-tracing/compare) with the implementation
