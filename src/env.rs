//! Environment-variable overrides for OTLP configuration.
//!
//! Allows `pheno-*` apps to configure the OTLP exporter without
//! recompiling — useful for 12-factor config and fleet-orchestration
//! (ADR-022). Env vars take precedence over explicit args.
//!
//! See ADR-012 (pheno-tracing canonical) for the policy.
//!
//! ## Recognized env vars
//!
//! | Variable                       | Default                       | Purpose                          |
//! |--------------------------------|-------------------------------|----------------------------------|
//! | `OTEL_COLLECTOR_GRPC_ENDPOINT` | `http://localhost:4317`       | OTLP/gRPC collector URL          |
//! | `OTEL_SERVICE_NAME`            | compile-time `CARGO_PKG_NAME` | resource.service.name            |
//! | `OTEL_SERVICE_VERSION`         | compile-time `CARGO_PKG_VERSION` | resource.service.version      |
//! | `OTEL_EXPORTER_OTLP_TIMEOUT`   | `10s`                         | per-export timeout               |
//! | `RUST_LOG`                     | `info`                        | tracing-subscriber env-filter    |

use std::time::Duration;

/// Resolved OTLP configuration from env (or defaults).
#[derive(Debug, Clone)]
pub struct OtlpEnvConfig {
    pub endpoint: String,
    pub service_name: String,
    pub service_version: String,
    pub timeout: Duration,
}

impl OtlpEnvConfig {
    /// Load from env, falling back to compile-time defaults.
    pub fn load() -> Self {
        let endpoint = std::env::var("OTEL_COLLECTOR_GRPC_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());
        let service_name = std::env::var("OTEL_SERVICE_NAME")
            .unwrap_or_else(|_| env!("CARGO_PKG_NAME").to_string());
        let service_version = std::env::var("OTEL_SERVICE_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
        let timeout_secs = std::env::var("OTEL_EXPORTER_OTLP_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(10);
        Self {
            endpoint,
            service_name,
            service_version,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

impl Default for OtlpEnvConfig {
    fn default() -> Self {
        Self::load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_no_env() {
        let cfg = OtlpEnvConfig::load();
        assert_eq!(cfg.endpoint, "http://localhost:4317");
        // service_name defaults to CARGO_PKG_NAME = "pheno-tracing"
        assert_eq!(cfg.service_name, "pheno-tracing");
        assert_eq!(cfg.timeout, Duration::from_secs(10));
    }

    #[test]
    fn endpoint_override() {
        // SAFETY: tests are not run in parallel for this test (no serial_test here
        // to avoid adding deps; if your fleet runs tests in parallel, gate with
        // `serial_test::serial`).
        std::env::set_var("OTEL_COLLECTOR_GRPC_ENDPOINT", "http://otel:4317");
        let cfg = OtlpEnvConfig::load();
        assert_eq!(cfg.endpoint, "http://otel:4317");
        std::env::remove_var("OTEL_COLLECTOR_GRPC_ENDPOINT");
    }

    #[test]
    fn service_name_override() {
        std::env::set_var("OTEL_SERVICE_NAME", "pheno-flags");
        let cfg = OtlpEnvConfig::load();
        assert_eq!(cfg.service_name, "pheno-flags");
        std::env::remove_var("OTEL_SERVICE_NAME");
    }

    #[test]
    fn timeout_parse() {
        std::env::set_var("OTEL_EXPORTER_OTLP_TIMEOUT", "30");
        let cfg = OtlpEnvConfig::load();
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        std::env::remove_var("OTEL_EXPORTER_OTLP_TIMEOUT");
    }

    #[test]
    fn invalid_timeout_falls_back() {
        std::env::set_var("OTEL_EXPORTER_OTLP_TIMEOUT", "not-a-number");
        let cfg = OtlpEnvConfig::load();
        assert_eq!(cfg.timeout, Duration::from_secs(10));
        std::env::remove_var("OTEL_EXPORTER_OTLP_TIMEOUT");
    }
}
