//! OTLP smoke tests — verify standard OpenTelemetry crate wiring compiles
//! and honors env-var configuration without requiring a live collector.

#![cfg(feature = "otlp")]

use pheno_tracing::otlp::{build_span_exporter, build_tracer_provider, OtlpError};

#[test]
fn otlp_smoke_span_exporter_builds() {
    let exporter = build_span_exporter();
    assert!(
        exporter.is_ok(),
        "span exporter must build: {:?}",
        exporter.err()
    );
}

#[test]
fn otlp_smoke_honors_otlp_endpoint_env() {
    // CI step sets OTEL_EXPORTER_OTLP_ENDPOINT before `--no-run`; exercise it here too.
    std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:4317");
    let exporter = build_span_exporter();
    std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    assert!(
        exporter.is_ok(),
        "exporter must build with OTEL_EXPORTER_OTLP_ENDPOINT set: {:?}",
        exporter.err()
    );
}

#[test]
fn otlp_smoke_tracer_provider_wires() {
    let provider = build_tracer_provider("otlp-smoke");
    match provider {
        Ok(p) => {
            let _tracer = opentelemetry::trace::TracerProvider::tracer(&p, "otlp-smoke");
        }
        Err(OtlpError::Exporter(msg)) => {
            panic!("tracer provider exporter failed: {msg}");
        }
        Err(e) => panic!("unexpected otlp error: {e}"),
    }
}
