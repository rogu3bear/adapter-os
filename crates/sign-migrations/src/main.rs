use anyhow::{Context, Result};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
struct SignatureData {
    hash: String,
    signature: String,
    algorithm: String,
    hash_algorithm: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SignaturesFile {
    schema_version: String,
    signed_at: String,
    public_key: String,
    signatures: HashMap<String, SignatureData>,
}

fn main() -> Result<()> {
    println!("AdapterOS Migration Signing Tool (Rust)");
    println!("========================================\n");

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let migrations_dir = project_root.join("migrations");
    let signatures_file = migrations_dir.join("signatures.json");
    let key_file = project_root.join("var").join("migration_signing_key_rust.bin");

    // Load or generate signing key
    let signing_key = if key_file.exists() {
        println!("✓ Loading existing signing key: {}", key_file.display());
        let key_bytes = fs::read(&key_file)
            .context("Failed to read signing key")?;
        SigningKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .context("Invalid key length")?,
        )
    } else {
        println!("Generating new Ed25519 signing key...");
        fs::create_dir_all(key_file.parent().unwrap())?;

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        fs::write(&key_file, signing_key.to_bytes())
            .context("Failed to write signing key")?;

        println!("✓ Key generated: {}", key_file.display());
        println!("⚠  Keep this key secure - required for CAB promotion\n");

        signing_key
    };

    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let public_key_base64 = base64::encode(verifying_key.as_bytes());

    println!("✓ Public key: {}\n", &public_key_base64[..32]);
    println!("Signing migrations...\n");

    // Collect all migration files
    let mut migration_files: Vec<PathBuf> = fs::read_dir(&migrations_dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "sql" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    migration_files.sort();

    // Sign each migration
    let mut signatures = HashMap::new();
    let mut count = 0;

    for migration_file in &migration_files {
        let filename = migration_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // Compute BLAKE3 hash
        let file_contents = fs::read(migration_file)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&file_contents);
        let file_hash = hasher.finalize();
        let file_hash_hex = file_hash.to_hex().to_string();

        // Sign the hash
        let signature = signing_key.sign(file_hash.as_bytes());
        let signature_base64 = base64::encode(signature.to_bytes());

        signatures.insert(
            filename.clone(),
            SignatureData {
                hash: file_hash_hex,
                signature: signature_base64,
                algorithm: "ed25519".to_string(),
                hash_algorithm: "blake3".to_string(),
            },
        );

        println!("  ✓ {}", filename);
        count += 1;
    }

    // Create signatures file
    let signatures_data = SignaturesFile {
        schema_version: "1.0".to_string(),
        signed_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        public_key: public_key_base64,
        signatures,
    };

    let json = serde_json::to_string_pretty(&signatures_data)?;
    fs::write(&signatures_file, json)
        .context("Failed to write signatures file")?;

    println!("\n✓ Successfully signed {} migrations", count);
    println!("✓ Signatures written to: {}", signatures_file.display());

    // Verify signatures
    println!("\nVerifying signatures...");
    let mut verify_count = 0;

    for migration_file in &migration_files {
        let filename = migration_file.file_name().unwrap().to_str().unwrap();
        let sig_data = &signatures_data.signatures[filename];

        // Recompute hash
        let file_contents = fs::read(migration_file)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(&file_contents);
        let file_hash = hasher.finalize();

        // Verify signature
        let signature_bytes = base64::decode(&sig_data.signature)?;
        let signature = ed25519_dalek::Signature::from_bytes(
            signature_bytes.as_slice().try_into()?,
        );

        use ed25519_dalek::Verifier;
        if verifying_key.verify(file_hash.as_bytes(), &signature).is_ok() {
            verify_count += 1;
        } else {
            println!("✗ Signature verification failed for {}", filename);
        }
    }

    println!("✓ Verified {}/{} signatures\n", verify_count, count);

    if verify_count == count {
        println!("All migrations successfully signed and verified!");
        Ok(())
    } else {
        anyhow::bail!("Some signatures failed verification");
    }
}
