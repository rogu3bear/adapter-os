//! SBOM generation from Cargo.lock

use adapteros_sbom::SpdxDocument;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Generate SBOM from Cargo.lock
pub fn generate_sbom() -> Result<()> {
    println!("Generating SBOM from Cargo.lock...");

    let workspace_root = find_workspace_root()?;
    let cargo_lock_path = workspace_root.join("Cargo.lock");

    if !cargo_lock_path.exists() {
        anyhow::bail!("Cargo.lock not found. Run 'cargo build' first.");
    }

    // Parse Cargo.lock
    let cargo_lock_content =
        fs::read_to_string(&cargo_lock_path).context("Failed to read Cargo.lock")?;

    let lock: cargo_lock::Lockfile = cargo_lock_content
        .parse()
        .context("Failed to parse Cargo.lock")?;

    // Create SPDX document
    let namespace = format!(
        "https://github.com/rogu3bear/adapter-os/sbom/{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );

    let mut doc = SpdxDocument::new("adapterOS".to_string(), namespace);

    // Add all packages from Cargo.lock
    for package in &lock.packages {
        let version = package.version.to_string();
        doc.add_package(package.name.to_string(), version);
    }

    // Add local crates with BLAKE3 hashes
    let crates_dir = workspace_root.join("crates");
    if crates_dir.exists() {
        for entry in fs::read_dir(crates_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let crate_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");

                // Compute hash of all .rs files in crate
                let hash = hash_crate_sources(&path)?;

                // Add as a file entry with hash
                doc.add_file(format!("crates/{}/src/lib.rs", crate_name), &hash);
            }
        }
    }

    // Validate SBOM
    doc.validate()?;

    // Serialize to JSON
    let json = doc.to_json()?;

    // Write to target/sbom.spdx.json for local validation
    let output_dir = workspace_root.join("target");
    fs::create_dir_all(&output_dir)?;

    let output_path = output_dir.join("sbom.spdx.json");
    fs::write(&output_path, &json).context("Failed to write SBOM")?;

    println!("✓ SBOM generated: {}", output_path.display());

    // Mirror into tracked snapshot to keep CI and lockfile in sync
    let tracked_dir = workspace_root.join("sbom");
    fs::create_dir_all(&tracked_dir)?;
    let tracked_path = tracked_dir.join("cargo-sbom.json");
    fs::write(&tracked_path, &json).context("Failed to write tracked SBOM")?;
    println!("✓ SBOM snapshot updated: {}", tracked_path.display());

    // Sign if key is present
    if let Ok(key_hex) = std::env::var("SBOM_SIGNING_KEY") {
        sign_sbom(&output_path, &key_hex)?;
    } else {
        println!("  (No SBOM_SIGNING_KEY found, skipping signature)");
    }

    Ok(())
}

/// Find workspace root by looking for Cargo.toml with [workspace]
fn find_workspace_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(current);
            }
        }

        if !current.pop() {
            anyhow::bail!("Could not find workspace root");
        }
    }
}

/// Hash all source files in a crate
fn hash_crate_sources(crate_path: &Path) -> Result<adapteros_core::B3Hash> {
    use adapteros_core::B3Hash;

    let mut hasher = blake3::Hasher::new();

    let src_dir = crate_path.join("src");
    if src_dir.exists() {
        let mut files = Vec::new();
        collect_rust_files(&src_dir, &mut files)?;

        // Sort for determinism
        files.sort();

        for file in files {
            let content = fs::read(&file)?;
            hasher.update(&content);
        }
    }

    let hash_bytes: [u8; 32] = hasher.finalize().into();
    Ok(B3Hash::new(hash_bytes))
}

/// Recursively collect .rs files
fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            files.push(path);
        }
    }

    Ok(())
}

/// Sign SBOM with Ed25519 key
fn sign_sbom(sbom_path: &Path, key_hex: &str) -> Result<()> {
    use ed25519_dalek::{Signer, SigningKey};

    println!("Signing SBOM with Ed25519...");

    // Decode signing key
    let key_bytes = hex::decode(key_hex).context("Invalid SBOM_SIGNING_KEY hex")?;

    if key_bytes.len() != 32 {
        anyhow::bail!("SBOM_SIGNING_KEY must be 32 bytes (64 hex chars)");
    }

    let signing_key = SigningKey::from_bytes(&key_bytes.try_into().unwrap());

    // Read SBOM
    let sbom_content = fs::read(sbom_path)?;

    // Sign
    let signature = signing_key.sign(&sbom_content);

    // Write signature file
    let sig_path = sbom_path.with_extension("spdx.json.sig");
    fs::write(&sig_path, signature.to_bytes()).context("Failed to write signature")?;

    println!("✓ Signature written: {}", sig_path.display());

    Ok(())
}

// Stub chrono for timestamp
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> Self {
            Self
        }
        pub fn format(&self, _fmt: &str) -> impl std::fmt::Display {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }
    }
}
