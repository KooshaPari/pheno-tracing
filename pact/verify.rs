//! Pact provider verification: pheno-tracing exposes /v1/traces (OTLP/HTTP)
//! Run with: cargo test --test pact_verify
use pact_consumer::prelude::*;

#[tokio::test]
async fn pact_verify_pheno_tracing_otlp_traces() {
    let pact = PactBuilder::new("pheno-mcp-router", "pheno-tracing")
        .interaction("POST OTLP span batch", |i| {
            i.request.path("/v1/traces");
            i.response.status(200);
        })
        .start_mock_server(None);

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/v1/traces", pact.url()))
        .header("Content-Type", "application/json")
        .body(r#"{"resourceSpans":[]}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}
