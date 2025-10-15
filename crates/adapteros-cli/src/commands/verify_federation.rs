//! Verify federation bundle signatures

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_verify::verify_cross_host;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct FederationVerificationResult {
    pub total_bundles: usize,
    pub total_signatures: usize,
    pub verified: bool,
    pub errors: Vec<String>,
}

/// Verify cross-host federation signatures
pub async fn run(bundle_dir: &Path, database: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!("Verifying federation chain: {}", bundle_dir.display()));
    
    // Connect to database
    output.progress("Connecting to database");
    let db = Db::connect(database.to_str().unwrap())
        .await
        .context("Failed to connect to database")?;
    
    // Run migrations to ensure federation tables exist
    db.migrate()
        .await
        .context("Failed to run database migrations")?;
    
    output.progress_done(true);
    
    // Verify cross-host chain
    output.progress("Verifying cross-host signatures");
    
    match verify_cross_host(bundle_dir, &db).await {
        Ok(_) => {
            output.progress_done(true);
            output.success("Federation chain verification successful");
            
            if output.is_json() {
                let result = FederationVerificationResult {
                    total_bundles: 0, // Would be populated from actual verification
                    total_signatures: 0,
                    verified: true,
                    errors: vec![],
                };
                output.json(&result)?;
            }
        }
        Err(e) => {
            output.progress_done(false);
            output.error(format!("Federation chain verification failed: {}", e));
            
            if output.is_json() {
                let result = FederationVerificationResult {
                    total_bundles: 0,
                    total_signatures: 0,
                    verified: false,
                    errors: vec![e.to_string()],
                };
                output.json(&result)?;
            }
            
            return Err(anyhow::anyhow!("Federation verification failed"));
        }
    }
    
    Ok(())
}

