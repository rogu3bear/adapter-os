//! Patch proposal API example
//!
//! This example demonstrates how to use the patch proposal system via the REST API
//! for generating code patches with evidence citations and policy validation.

use reqwest::Client;
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 AdapterOS Patch Proposal System - API Example");
    println!("================================================");

    let client = Client::new();
    let base_url = "http://localhost:8080";

    // 1. Health check
    println!("🔍 Checking service health...");
    let health_response = client.get(&format!("{}/healthz", base_url)).send().await?;

    if health_response.status().is_success() {
        println!("✅ Service is healthy");
    } else {
        println!("❌ Service health check failed");
        return Ok(());
    }

    // 2. Authentication (mock token for example)
    let auth_token = "mock_jwt_token_here";
    println!("🔐 Using mock authentication token");

    // 3. Create patch proposal request
    let patch_request = json!({
        "repo_id": "auth_service",
        "commit_sha": "def456",
        "description": "Add JWT authentication middleware with proper error handling and logging",
        "target_files": ["src/middleware/mod.rs"]
    });

    println!("📝 Creating patch proposal...");
    println!("   Repository: {}", patch_request["repo_id"]);
    println!("   Commit: {}", patch_request["commit_sha"]);
    println!("   Description: {}", patch_request["description"]);
    println!("   Target files: {:?}", patch_request["target_files"]);

    // 4. Submit patch proposal request
    let response = client
        .post(&format!("{}/v1/propose-patch", base_url))
        .header("Authorization", format!("Bearer {}", auth_token))
        .header("Content-Type", "application/json")
        .json(&patch_request)
        .send()
        .await?;

    let status = response.status();

    if status.is_success() {
        let patch_response: serde_json::Value = response.json().await?;
        println!("✅ Patch proposal created successfully!");
        println!("   Proposal ID: {}", patch_response["proposal_id"]);
        println!("   Status: {}", patch_response["status"]);
        println!("   Message: {}", patch_response["message"]);
    } else {
        let error_response: serde_json::Value = response.json().await?;
        println!("❌ Patch proposal failed");
        println!("   Status: {}", status);
        println!("   Error: {}", error_response["error"]);
        if let Some(details) = error_response.get("details") {
            println!("   Details: {}", details);
        }
    }

    // 5. Example with different scenarios
    println!("\n🎯 Testing different scenarios...");

    let scenarios = vec![
        (
            "Database Migration",
            json!({
                "repo_id": "user_service",
                "commit_sha": "ghi789",
                "description": "Add user profiles table with foreign key to users and proper indexes",
                "target_files": ["migrations/002_add_user_profiles.sql"]
            }),
        ),
        (
            "API Endpoint",
            json!({
                "repo_id": "user_api",
                "commit_sha": "jkl012",
                "description": "Add POST /api/posts endpoint with input validation, rate limiting, and proper error responses",
                "target_files": ["src/api/posts.rs"]
            }),
        ),
        (
            "Performance Optimization",
            json!({
                "repo_id": "performance_service",
                "commit_sha": "mno345",
                "description": "Optimize user search endpoint with caching, database indexing, and query optimization",
                "target_files": ["src/api/search.rs"]
            }),
        ),
        (
            "Security Fix",
            json!({
                "repo_id": "secure_service",
                "commit_sha": "pqr678",
                "description": "Fix SQL injection vulnerability in user lookup query by using parameterized statements",
                "target_files": ["src/db/user_queries.rs"]
            }),
        ),
    ];

    for (scenario_name, scenario_request) in scenarios {
        println!("\n📋 Scenario: {}", scenario_name);

        let response = client
            .post(format!("{}/v1/propose-patch", base_url))
            .header("Authorization", format!("Bearer {}", auth_token))
            .header("Content-Type", "application/json")
            .json(&scenario_request)
            .send()
            .await?;

        if response.status().is_success() {
            let patch_response: serde_json::Value = response.json().await?;
            println!("   ✅ Success: {}", patch_response["status"]);
            println!("   📝 Message: {}", patch_response["message"]);
        } else {
            let error_response: serde_json::Value = response.json().await?;
            println!("   ❌ Failed: {}", error_response["error"]);
        }
    }

    // 6. Error handling examples
    println!("\n🚨 Testing error handling...");

    let error_scenarios = vec![
        (
            "Invalid Repository",
            json!({
                "repo_id": "",
                "commit_sha": "abc123",
                "description": "Test invalid repo",
                "target_files": ["src/test.rs"]
            }),
        ),
        (
            "Missing Description",
            json!({
                "repo_id": "test_repo",
                "commit_sha": "abc123",
                "description": "",
                "target_files": ["src/test.rs"]
            }),
        ),
        (
            "Invalid File Path",
            json!({
                "repo_id": "test_repo",
                "commit_sha": "abc123",
                "description": "Test invalid path",
                "target_files": ["../../../etc/passwd"]
            }),
        ),
    ];

    for (error_name, error_request) in error_scenarios {
        println!("\n🔍 Error Test: {}", error_name);

        let response = client
            .post(format!("{}/v1/propose-patch", base_url))
            .header("Authorization", format!("Bearer {}", auth_token))
            .header("Content-Type", "application/json")
            .json(&error_request)
            .send()
            .await?;

        if response.status().is_client_error() {
            let error_response: serde_json::Value = response.json().await?;
            println!("   ✅ Correctly rejected: {}", error_response["error"]);
        } else {
            println!("   ⚠️  Unexpected success for error scenario");
        }
    }

    // 7. Performance testing
    println!("\n⚡ Performance testing...");

    let start_time = std::time::Instant::now();
    let mut success_count = 0;
    let mut error_count = 0;

    for i in 0..5 {
        let perf_request = json!({
            "repo_id": format!("perf_test_{}", i),
            "commit_sha": "perf123",
            "description": format!("Performance test patch {}", i),
            "target_files": ["src/perf_test.rs"]
        });

        let response = client
            .post(&format!("{}/v1/propose-patch", base_url))
            .header("Authorization", format!("Bearer {}", auth_token))
            .header("Content-Type", "application/json")
            .json(&perf_request)
            .send()
            .await?;

        if response.status().is_success() {
            success_count += 1;
        } else {
            error_count += 1;
        }
    }

    let total_time = start_time.elapsed();
    println!("   📊 Results:");
    println!("     Total time: {:?}", total_time);
    println!("     Average time: {:?}", total_time / 5);
    println!("     Successes: {}", success_count);
    println!("     Errors: {}", error_count);

    println!("\n🎉 API example completed successfully!");
    Ok(())
}

// Helper function to create a mock JWT token (for testing only)
fn create_mock_jwt_token() -> String {
    // In a real implementation, this would create a proper JWT token
    // with the correct claims and signature
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0X3VzZXIiLCJyb2xlIjoiT3BlcmF0b3IiLCJleHAiOjE2NDA5OTUyMDB9.mock_signature".to_string()
}
