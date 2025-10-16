//! Run audit checks

use adapteros_db::Db;
use adapteros_policy::CodeMetrics;
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

use crate::output::OutputWriter;

pub async fn run(cpid: &str, suite: Option<&Path>, output: &OutputWriter) -> Result<()> {
    output.section(format!("Running audit for CPID: {}", cpid));

    let suite_path = suite
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| "tests/corpora/reg_v1.json".into());

    output.info(format!("Using test suite: {}", suite_path.display()));

    // Connect to database
    let db = Db::connect_env()
        .await
        .context("Failed to connect to database")?;

    // Create audit job
    let payload = serde_json::json!({
        "cpid": cpid,
        "suite_path": suite_path.to_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?,
    });

    let job_id = db
        .create_job(
            "audit",
            None, // tenant_id determined from cpid
            None,
            &payload.to_string(),
        )
        .await
        .context("Failed to create audit job")?;

    output.success(format!("Audit job created: {}", job_id));
    output.info("Waiting for job to complete...");

    // Poll for job completion
    let mut attempts = 0;
    let max_attempts = 180; // 6 minutes with 2-second intervals

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let job = db
            .get_job(&job_id)
            .await
            .context("Failed to get job status")?
            .ok_or_else(|| anyhow::anyhow!("Job not found"))?;

        match job.status.as_str() {
            "finished" => {
                let result: serde_json::Value =
                    serde_json::from_str(job.result_json.as_deref().unwrap_or("{}"))?;

                if let Some(data) = result["data"].as_object() {
                    println!("\n✓ Audit completed");
                    println!(
                        "  Tests: {}/{} passed",
                        data["passed"].as_u64().unwrap_or(0),
                        data["total_tests"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  Avg latency: {:.2} ms",
                        data["avg_latency_ms"].as_f64().unwrap_or(0.0)
                    );
                    println!(
                        "  P95 latency: {:.2} ms",
                        data["p95_latency_ms"].as_f64().unwrap_or(0.0)
                    );
                    let determinism_pass = data["determinism_pass"].as_bool().unwrap_or(false);
                    println!(
                        "  Determinism: {}",
                        if determinism_pass { "PASS" } else { "FAIL" }
                    );

                    // Show determinism hash if available
                    if determinism_pass {
                        if let Some(hash) = data["determinism_hash"].as_str() {
                            output.success(format!("Determinism verified: {}", hash));
                        }
                    }

                    if let Some(metrics) = data["hallucination_metrics"].as_object() {
                        println!("\n  Hallucination Metrics:");
                        println!("    ARR:  {:.2}", metrics["arr"].as_f64().unwrap_or(0.0));
                        println!("    ECS5: {:.2}", metrics["ecs5"].as_f64().unwrap_or(0.0));
                        println!("    HLR:  {:.3}", metrics["hlr"].as_f64().unwrap_or(0.0));
                        println!("    CR:   {:.3}", metrics["cr"].as_f64().unwrap_or(0.0));
                    }

                    // Display code-specific metrics if present
                    if let Some(code_metrics) = data["code_metrics"].as_object() {
                        println!("\n  Code Intelligence Metrics:");

                        if let Some(csr) = code_metrics["csr"].as_f64() {
                            println!("    CSR (Compile Success Rate): {:.2}%", csr * 100.0);
                        }

                        if let Some(test_pass1) = code_metrics["test_pass1"].as_f64() {
                            println!("    Test Pass@1: {:.2}%", test_pass1 * 100.0);
                        }

                        if let Some(arr) = code_metrics["arr"].as_f64() {
                            println!("    ARR (Answer Relevance): {:.2}%", arr * 100.0);
                        }

                        // Check code metric thresholds
                        let csr_threshold = 0.9;
                        let test_threshold = 0.8;
                        let arr_threshold = 0.95;

                        if let (Some(csr), Some(tp1), Some(arr)) = (
                            code_metrics["csr"].as_f64(),
                            code_metrics["test_pass1"].as_f64(),
                            code_metrics["arr"].as_f64(),
                        ) {
                            println!("\n  Code Metric Gates:");
                            println!(
                                "    CSR >= {:.0}%: {}",
                                csr_threshold * 100.0,
                                if csr >= csr_threshold {
                                    "✓ PASS"
                                } else {
                                    "✗ FAIL"
                                }
                            );
                            println!(
                                "    Test Pass@1 >= {:.0}%: {}",
                                test_threshold * 100.0,
                                if tp1 >= test_threshold {
                                    "✓ PASS"
                                } else {
                                    "✗ FAIL"
                                }
                            );
                            println!(
                                "    ARR >= {:.0}%: {}",
                                arr_threshold * 100.0,
                                if arr >= arr_threshold {
                                    "✓ PASS"
                                } else {
                                    "✗ FAIL"
                                }
                            );
                        }
                    }

                    // Check if audit passed
                    let passed = data["passed"].as_u64().unwrap_or(0);
                    let total = data["total_tests"].as_u64().unwrap_or(1);

                    if passed == total {
                        return Ok(());
                    } else {
                        return Err(anyhow::anyhow!(
                            "Audit failed: {}/{} tests passed",
                            passed,
                            total
                        ));
                    }
                }

                return Ok(());
            }
            "failed" => {
                let result_json = job.result_json.as_deref().unwrap_or("{}");
                let result: serde_json::Value = serde_json::from_str(result_json)?;
                let error_msg = result["message"].as_str().unwrap_or("Unknown error");

                return Err(anyhow::anyhow!("Audit job failed: {}", error_msg));
            }
            "running" => {
                output.verbose(".");
            }
            _ => {}
        }

        attempts += 1;
        if attempts >= max_attempts {
            return Err(anyhow::anyhow!("Audit job timed out"));
        }
    }
}

/// Generate code metrics report from audit data
pub fn generate_code_metrics_report(
    cpid: &str,
    code_metrics: &CodeMetrics,
    output_path: &Path,
) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

    let summary = code_metrics.summary();

    let report = format!(
        "# Code Intelligence Audit Report\n\n\
         **CPID:** {}\n\
         **Generated:** {}\n\n\
         ## Metrics Summary\n\n\
         ### Compile Success Rate (CSR)\n\
         - **Rate:** {:.2}%\n\
         - **Successful:** {}\n\
         - **Total:** {}\n\n\
         ### Test Pass@1\n\
         - **Rate:** {:.2}%\n\
         - **Passed First Try:** {}\n\
         - **Total Runs:** {}\n\n\
         ### Answer Relevance Rate (ARR)\n\
         - **Rate:** {:.2}%\n\
         - **With Citations:** {}\n\
         - **Total Responses:** {}\n\n\
         ## Gate Status\n\n\
         | Metric | Threshold | Actual | Status |\n\
         |--------|-----------|--------|--------|\n\
         | CSR | ≥ 90% | {:.1}% | {} |\n\
         | Test Pass@1 | ≥ 80% | {:.1}% | {} |\n\
         | ARR | ≥ 95% | {:.1}% | {} |\n\n\
         ## Recommendations\n\n",
        cpid,
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        summary.csr * 100.0,
        (summary.csr * summary.total_compiles as f32) as usize,
        summary.total_compiles,
        summary.test_pass1 * 100.0,
        (summary.test_pass1 * summary.total_tests as f32) as usize,
        summary.total_tests,
        summary.arr * 100.0,
        (summary.arr * summary.total_responses as f32) as usize,
        summary.total_responses,
        summary.csr * 100.0,
        if summary.csr >= 0.9 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        },
        summary.test_pass1 * 100.0,
        if summary.test_pass1 >= 0.8 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        },
        summary.arr * 100.0,
        if summary.arr >= 0.95 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        },
    );

    let mut recommendations = String::new();

    if summary.csr < 0.9 {
        recommendations.push_str(
            "- **CSR below threshold:** Review linter integration and compilation checks\n",
        );
    }

    if summary.test_pass1 < 0.8 {
        recommendations.push_str("- **Test Pass@1 below threshold:** Improve test selection and code generation quality\n");
    }

    if summary.arr < 0.95 {
        recommendations.push_str(
            "- **ARR below threshold:** Enhance evidence retrieval and citation generation\n",
        );
    }

    if recommendations.is_empty() {
        recommendations
            .push_str("All metrics meet thresholds. Excellent code intelligence performance!\n");
    }

    let full_report = format!("{}{}\n", report, recommendations);

    let mut file = File::create(output_path).context("Failed to create report file")?;

    file.write_all(full_report.as_bytes())
        .context("Failed to write report")?;

    println!("✓ Code metrics report generated: {}", output_path.display());

    Ok(())
}
