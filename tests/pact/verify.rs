//! L27 Pact provider verification (v20 cycle-10 T4) — pheno-tracing.
//!
//! This is a `cargo test` entry point. The actual test logic lives
//! in `pact/verify.rs` (the spec path); this file `include!`s it so
//! it shows up under `cargo test -p pheno-tracing pact::verify`.
//!
//! See `pact/verify.rs` for the full rationale on the in-process
//! `InMemoryAdapter` test instance and the future HTTP shim.

include!("../../pact/verify.rs");
