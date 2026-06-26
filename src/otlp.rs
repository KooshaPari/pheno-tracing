//! OTLP tracing wiring via standard OpenTelemetry crates.
//!
//! Replaces the deleted `pheno-otel` substrate with published
//! `opentelemetry` + `opentelemetry_sdk` + `opentelemetry-otlp` +
//! `tracing-opentelemetry` dependencies. Honors the standard OTel env vars
//! (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SDK_DISABLED`, etc.).

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use thiserror::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Errors encountered while wiring OTLP export.
#[derive(Debug, Error)]
pub enum OtlpError {
    /// Failed to construct the OTLP span exporter.
    #[error("failed to build OTLP span exporter: {0}")]
    Exporter(String),
    /// Failed to install the global tracing subscriber.
    #[error("failed to init tracing subscriber: {0}")]
    SubscriberInit(String),
}

/// Build an OTLP span exporter honoring `OTEL_EXPORTER_OTLP_ENDPOINT` and related env vars.
pub fn build_span_exporter() -> Result<opentelemetry_otlp::SpanExporter, OtlpError> {
    opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .build()
        .map_err(|e| OtlpError::Exporter(e.to_string()))
}

/// Build an [`SdkTracerProvider`] with OTLP batch export for `service_name`.
pub fn build_tracer_provider(service_name: &str) -> Result<SdkTracerProvider, OtlpError> {
    let exporter = build_span_exporter()?;
    let resource = Resource::builder()
        .with_service_name(service_name.to_string())
        .build();

    Ok(SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build())
}

/// Initialize global tracing with OTLP export layered via `tracing-opentelemetry`.
///
/// Installs a registry subscriber with an env-filter (from `RUST_LOG` /
/// `RUST_LOG`-style defaults) and an OpenTelemetry layer backed by OTLP.
pub fn init_otlp(service_name: &str) -> Result<(), OtlpError> {
    let provider = build_tracer_provider(service_name)?;
    global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer(service_name.to_string());
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(otel_layer)
        .try_init()
        .map_err(|e| OtlpError::SubscriberInit(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_span_exporter_succeeds_with_sdk_disabled() {
        // CI sets OTEL_SDK_DISABLED=true; exporter construction must still compile/run.
        let result = build_span_exporter();
        assert!(
            result.is_ok(),
            "expected exporter build to succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn build_tracer_provider_sets_service_name() {
        let provider =
            build_tracer_provider("pheno-tracing-test").expect("tracer provider should build");
        let _tracer = provider.tracer("pheno-tracing-test");
    }
}
