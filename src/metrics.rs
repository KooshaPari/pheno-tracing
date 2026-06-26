//! Internal error observability (L62 error-rate adoption, v14 cycle-4 T7).

/// Record an internal error for fleet error-rate observability.
///
/// Emits a structured `tracing` event so downstream collectors (including
/// OTLP via `tracing-opentelemetry`) can attribute errors without a
/// separate metrics substrate dependency.
pub fn record_error(operation: &str, error_kind: &str) {
    tracing::error!(
        target: "pheno_tracing.metrics",
        operation,
        error_kind,
        "internal error recorded"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_error_emits_structured_event() {
        // Compile-time + runtime smoke: must not panic when no subscriber is installed.
        record_error("pheno_tracing.test", "smoke");
    }
}
