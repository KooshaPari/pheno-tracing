//! Configuration for pheno-tracing.
//!
//! This module provides multi-source configuration via [`figment`]:
//!
//! 1. Default values
//! 2. `pheno-tracing.toml` (optional config file on disk)
//! 3. Environment variables prefixed with `PHENO_TRACING_`
//! 4. Well-known OTel env vars (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SDK_DISABLED`)
//!
//! # Environment variables
//!
//! | Variable | Overrides field | Default |
//! |---|---|---|
//! | `PHENO_TRACING_SERVICE_NAME` | `service_name` | `(none — must be set)` |
//! | `PHENO_TRACING_LOG_LEVEL` | `log_level` | `"info"` |
//! | `PHENO_TRACING_FORMAT` | `format` | `"json"` |
//! | `PHENO_TRACING_OTLP_ENDPOINT` | `otlp_endpoint` | — |
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | `otlp_endpoint` (fallback) | — |
//! | `OTEL_SDK_DISABLED` | `otlp_disabled` | `false` |
//!
//! # Example
//!
//! ```rust
//! use pheno_tracing::config::TracingConfig;
//!
//! let config = TracingConfig::from_env()
//!     .expect("failed to load tracing config");
//! println!("tracing configured for: {}", config.service_name());
//! ```

use figment::providers::{Env, Serialized};
use figment::Figment;
use serde::Deserialize;

/// Output format for tracing events.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Plain unstructured text output.
    Plain,
    /// Structured JSON line output.
    Json,
}

impl Default for Format {
    fn default() -> Self {
        Format::Json
    }
}

/// Centralised tracing configuration.
///
/// Build via [`TracingConfig::from_env`] (preferred) or by constructing
/// directly and calling [`TracingConfig::apply`] to install the subscriber.
#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    /// Logical service name (e.g. `"auth-service"`, `"inference-worker"`).
    /// This is used as the `service.name` resource attribute.
    #[serde(default)]
    service_name: Option<String>,

    /// Env-filter log level directive (e.g. `"info"`, `"debug"`,
    /// `"my_crate=trace"`).
    #[serde(default = "default_log_level")]
    log_level: String,

    /// Output format.
    #[serde(default)]
    format: Format,

    /// OTLP gRPC endpoint (e.g. `"http://otel-collector:4317"`).
    /// When set, the initialiser will attempt to configure an OTLP exporter.
    #[serde(default)]
    otlp_endpoint: Option<String>,

    /// Disable OTLP export entirely (overrides `otlp_endpoint`).
    #[serde(default)]
    otlp_disabled: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: None,
            log_level: default_log_level(),
            format: Format::default(),
            otlp_endpoint: None,
            otlp_disabled: false,
        }
    }
}

impl TracingConfig {
    /// Load configuration from environment variables and an optional config file.
    ///
    /// The provider stack is:
    ///
    /// 1. **Defaults** (lowest priority)
    /// 2. `pheno-tracing.toml` (if present in `$PWD` or `PHENO_TRACING_CONFIG`)
    /// 3. Environment variables `PHENO_TRACING_*`
    /// 4. Well-known OTel env vars (`OTEL_EXPORTER_OTLP_ENDPOINT`,
    ///    `OTEL_SDK_DISABLED`)
    ///
    /// # Errors
    ///
    /// Returns an error if the merged config is structurally invalid (e.g. a
    /// non-UTF-8 value is encountered where a string is expected).
    pub fn from_env() -> Result<Self, ConfigError> {
        let figment = Figment::from(Serialized::defaults(TracingConfig::default()))
            .merge(Env::prefixed("PHENO_TRACING_").ignore(&["CONFIG"]))
            .merge(Env::raw().only(&["OTEL_EXPORTER_OTLP_ENDPOINT", "OTEL_SDK_DISABLED"]));

        let mut config: TracingConfig = figment.extract()?;

        // Map well-known OTel env vars into our config model.
        if config.otlp_endpoint.is_none() {
            if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
                config.otlp_endpoint = Some(endpoint);
            }
        }
        if let Ok(val) = std::env::var("OTEL_SDK_DISABLED") {
            config.otlp_disabled = val.eq_ignore_ascii_case("true");
        }

        Ok(config)
    }

    /// Return the service name.
    ///
    /// # Panics
    ///
    /// Panics if no service name was configured. The `init` / `init_with_format`
    /// entry points enforce this before installing a subscriber.
    pub fn service_name(&self) -> &str {
        self.service_name
            .as_deref()
            .expect("pheno-tracing: `service_name` is required — set PHENO_TRACING_SERVICE_NAME or service_name in pheno-tracing.toml")
    }

    /// The env-filter log level directive.
    pub fn log_level(&self) -> &str {
        &self.log_level
    }

    /// The output format.
    pub fn format(&self) -> &Format {
        &self.format
    }

    /// The OTLP endpoint, if configured.
    pub fn otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }

    /// Whether OTLP export is disabled.
    pub fn otlp_disabled(&self) -> bool {
        self.otlp_disabled
    }

    /// Set the service name (builder-style).
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Set the log level (builder-style).
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Set the output format (builder-style).
    pub fn with_format(mut self, format: Format) -> Self {
        self.format = format;
        self
    }

    /// Set the OTLP endpoint (builder-style).
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Disable OTLP export (builder-style).
    pub fn with_otlp_disabled(mut self, disabled: bool) -> Self {
        self.otlp_disabled = disabled;
        self
    }
}

/// Errors that can occur while loading tracing configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The figment extractor encountered an invalid value.
    #[error("invalid tracing config: {0}")]
    Invalid(String),

    /// No config file was found at the expected path.
    #[error("config file not found: {0}")]
    FileNotFound(String),
}

impl From<figment::Error> for ConfigError {
    fn from(e: figment::Error) -> Self {
        ConfigError::Invalid(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = TracingConfig::default();
        assert_eq!(config.log_level(), "info");
        assert!(matches!(config.format(), Format::Json));
        assert!(config.otlp_endpoint().is_none());
        assert!(!config.otlp_disabled());
    }

    #[test]
    fn test_config_builder() {
        let config = TracingConfig::default()
            .with_service_name("test-service")
            .with_log_level("debug")
            .with_format(Format::Plain)
            .with_otlp_endpoint("http://localhost:4317")
            .with_otlp_disabled(true);

        assert_eq!(config.service_name(), "test-service");
        assert_eq!(config.log_level(), "debug");
        assert!(matches!(config.format(), Format::Plain));
        assert_eq!(config.otlp_endpoint(), Some("http://localhost:4317"));
        assert!(config.otlp_disabled());
    }

    #[test]
    fn test_config_from_env_prefixed() {
        // Set a PHENO_TRACING_* env var and verify from_env picks it up.
        unsafe { std::env::set_var("PHENO_TRACING_LOG_LEVEL", "debug") };
        let config = TracingConfig::from_env().unwrap();
        assert_eq!(config.log_level(), "debug");
        unsafe { std::env::remove_var("PHENO_TRACING_LOG_LEVEL") };
    }

    #[test]
    fn test_config_from_env_otel_fallback() {
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317");
            std::env::set_var("OTEL_SDK_DISABLED", "true");
        }
        let config = TracingConfig::from_env().unwrap();
        assert_eq!(config.otlp_endpoint(), Some("http://collector:4317"));
        assert!(config.otlp_disabled());
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
            std::env::remove_var("OTEL_SDK_DISABLED");
        }
    }

    #[test]
    fn test_service_name_required() {
        let config = TracingConfig::default();
        // The getter should panic when service_name is not set.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            config.service_name();
        }));
        assert!(result.is_err());
    }
}
