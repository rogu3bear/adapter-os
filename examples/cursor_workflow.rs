//! Example: Cursor IDE integration workflow
//!
//! Demonstrates:
//! 1. Register repository
//! 2. Trigger scan
//! 3. Query status
//! 4. Subscribe to file changes

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("AdapterOS + Cursor Integration Example\n");

    // Step 1: Register repository
    println!("Step 1: Registering repository...");
    let repo_response = reqwest::Client::new()
        .post("http://localhost:8080/v1/code/register-repo")
        .json(&serde_json::json!({
            "tenant_id": "default",
            "repo_id": "example-project",
            "path": "/Users/dev/example-project",
            "languages": ["Rust", "Python"],
            "default_branch": "main"
        }))
        .send()
        .await?;

    if repo_response.status().is_success() {
        println!("✓ Repository registered");
    } else {
        println!("⚠ Repository already exists or error occurred");
    }

    // Step 2: Trigger scan
    println!("\nStep 2: Triggering repository scan...");
    let scan_response = reqwest::Client::new()
        .post("http://localhost:8080/v1/code/scan")
        .json(&serde_json::json!({
            "tenant_id": "default",
            "repo_id": "example-project",
            "commit": "HEAD",
            "full_scan": true
        }))
        .send()
        .await?;

    let scan_result: serde_json::Value = scan_response.json().await?;
    let job_id = scan_result["job_id"]
        .as_str()
        .expect("job_id should be present");

    println!("✓ Scan job created: {}", job_id);

    // Step 3: Poll for completion
    println!("\nStep 3: Waiting for scan to complete...");
    let mut last_progress = 0;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let status_response = reqwest::Client::new()
            .get(format!("http://localhost:8080/v1/code/scan/{}", job_id))
            .send()
            .await?;

        let status: serde_json::Value = status_response.json().await?;
        let job_status = status["status"].as_str().unwrap_or("unknown");
        let progress = status["progress"]["percentage"].as_i64().unwrap_or(0) as i32;

        if progress > last_progress {
            let stage = status["progress"]["current_stage"]
                .as_str()
                .unwrap_or("processing");
            println!("  Progress: {}% ({})", progress, stage);
            last_progress = progress;
        }

        match job_status {
            "completed" => {
                println!("✓ Scan completed!");
                if let Some(result) = status["result"].as_object() {
                    let files = result["file_count"].as_i64().unwrap_or(0);
                    let symbols = result["symbol_count"].as_i64().unwrap_or(0);
                    println!("  Files: {}, Symbols: {}", files, symbols);
                }
                break;
            }
            "failed" => {
                let error = status["error_message"].as_str().unwrap_or("Unknown error");
                println!("✗ Scan failed: {}", error);
                break;
            }
            _ => {
                // Continue polling
            }
        }
    }

    // Step 4: Query repository status
    println!("\nStep 4: Querying repository status...");
    let repo_status = reqwest::Client::new()
        .get("http://localhost:8080/v1/code/repositories/example-project")
        .query(&[("tenant_id", "default")])
        .send()
        .await?;

    let repo_data: serde_json::Value = repo_status.json().await?;
    println!("Repository Status:");
    println!(
        "  Repo ID: {}",
        repo_data["repo_id"].as_str().unwrap_or("unknown")
    );
    println!(
        "  Status: {}",
        repo_data["status"].as_str().unwrap_or("unknown")
    );
    println!(
        "  Latest Scan: {}",
        repo_data["latest_scan_commit"].as_str().unwrap_or("none")
    );

    // Step 5: Demonstrate file change streaming
    println!("\nStep 5: File change streaming available at:");
    println!("  http://localhost:8080/v1/streams/file-changes?repo_id=example-project");
    println!("\nUse curl to subscribe:");
    println!("  curl -N http://localhost:8080/v1/streams/file-changes?repo_id=example-project");

    println!("\n✓ Cursor integration workflow complete!");
    println!("\nNext steps:");
    println!("  1. Configure manifest with code features");
    println!("  2. Build and serve plan with code adapters");
    println!("  3. Issue inference requests with code context");
    println!("  4. Verify evidence-grounded responses");

    Ok(())
}
