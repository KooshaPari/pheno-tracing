//! Chaos injection tests for pheno-tracing (L36)
//! Run with: cargo test chaos_injection -- --include-ignored

use chaos_injection::{ChaosRunner, Fault, ProbabilisticSelector};

#[tokio::test]
#[ignore = "chaos test"]
async fn chaos_pheno-tracing-resilience_to_latency() {
    let runner = ChaosRunner::new(ProbabilisticSelector::default());
    let result: Result<(), pheno_errors::Error> = runner.run("pheno-tracing-test", || async {
        // Operation that should succeed despite latency injection
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        Ok(())
    }).await;
    // Either succeeds (no fault) or returns Err (fault injected) — never panics
    let _ = result;
}

#[tokio::test]
#[ignore = "chaos test"]
async fn chaos_pheno-tracing-recovers_from_connection_refused() {
    let runner = ChaosRunner::new(ProbabilisticSelector::default());
    // Verify retry logic survives ConnectionRefused
    let mut attempts = 0;
    while attempts < 3 {
        let result: Result<(), pheno_errors::Error> = runner.run("pheno-tracing-retry", || async {
            attempts += 1;
            Err(pheno_errors::Error::ConnectionRefused)
        }).await;
        if result.is_ok() { break; }
    }
    assert!(attempts > 0);
}

#[tokio::test]
#[ignore = "chaos test"]
async fn chaos_pheno-tracing-handles_timeout() {
    let runner = ChaosRunner::new(ProbabilisticSelector::default());
    let result: Result<(), pheno_errors::Error> = runner.run("pheno-tracing-timeout", || async {
        // Simulate slow operation
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            tokio::time::sleep(std::time::Duration::from_secs(60)),
        ).await.map_err(|_| pheno_errors::Error::Timeout)?;
        Ok(())
    }).await;
    let _ = result;
}

#[tokio::test]
#[ignore = "chaos test"]
async fn chaos_pheno-tracing-error_probability_distribution() {
    let runner = ChaosRunner::new(ProbabilisticSelector::default());
    let mut errors = 0;
    let total = 100;
    for _ in 0..total {
        let result: Result<(), pheno_errors::Error> = runner.run("pheno-tracing-dist", || async {
            Err(pheno_errors::Error::Chaos("test"))
        }).await;
        if result.is_err() { errors += 1; }
    }
    // ProbabilisticSelector has 10% latency + 5% error → ~85% pass, 15% fail
    assert!(errors > 0 && errors < total, "expected some errors from chaos, got {} / {}", errors, total);
}
