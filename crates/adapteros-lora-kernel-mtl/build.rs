use std::path::{Path, PathBuf};
use std::process::Command;

fn write_bytes_if_changed(path: &Path, bytes: &[u8]) {
    let should_write = match std::fs::read(path) {
        Ok(existing) => existing != bytes,
        Err(_) => true,
    };

    if !should_write {
        return;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create parent directory");
    }

    std::fs::write(path, bytes).expect("Failed to write file");
}

fn main() {
    // Only compile Metal shaders on macOS
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "macos" {
        println!("cargo:warning=Skipping Metal shader compilation on non-macOS platform");
        return;
    }

    // Compile CoreML bridge if feature is enabled
    #[cfg(feature = "coreml-backend")]
    {
        compile_coreml_bridge();
    }

    // Continue with Metal shader compilation
    compile_metal_shaders();

    // Generate and sign manifest with test keys
    generate_signed_manifest();
}

#[cfg(feature = "coreml-backend")]
fn compile_coreml_bridge() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-changed=src/coreml_bridge.mm");
    println!("cargo:rerun-if-changed=src/coreml_ffi.h");

    // Check if CoreML bridge files exist
    let coreml_bridge_path = PathBuf::from("src/coreml_bridge.mm");
    let coreml_ffi_header = PathBuf::from("src/coreml_ffi.h");

    if !coreml_bridge_path.exists() {
        println!("cargo:warning=CoreML bridge source file not found, skipping CoreML compilation");
        return;
    }

    if !coreml_ffi_header.exists() {
        println!("cargo:warning=CoreML FFI header not found, skipping CoreML compilation");
        return;
    }

    // Compile Objective-C++ bridge for CoreML
    cc::Build::new()
        .file("src/coreml_bridge.mm")
        .flag("-framework")
        .flag("CoreML")
        .flag("-framework")
        .flag("Foundation")
        .flag("-std=c++17")
        .flag("-fobjc-arc") // Enable Automatic Reference Counting
        .cpp(true)
        .compile("coreml_bridge");

    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:warning=CoreML bridge compiled successfully");
}

fn resolve_toolchain_bin(real_home: Option<&str>) -> Option<PathBuf> {
    if let Ok(bin_override) = std::env::var("METAL_TOOLCHAIN_BIN") {
        let candidate = PathBuf::from(bin_override);
        if candidate.join("metal").exists() && candidate.join("metallib").exists() {
            return Some(candidate);
        }
    }

    if let Some(home) = real_home {
        let candidate =
            Path::new(home).join("Library/Developer/Toolchains/Metal.xctoolchain/usr/bin");
        if candidate.join("metal").exists() && candidate.join("metallib").exists() {
            return Some(candidate);
        }
    }

    let system_candidate =
        Path::new("/Library/Developer/Toolchains/Metal.xctoolchain/usr/bin").to_path_buf();
    if system_candidate.join("metal").exists() && system_candidate.join("metallib").exists() {
        return Some(system_candidate);
    }

    let xcode_candidate = Path::new(
        "/Applications/Xcode.app/Contents/Developer/Toolchains/Metal.xctoolchain/usr/bin",
    )
    .to_path_buf();
    if xcode_candidate.join("metal").exists() && xcode_candidate.join("metallib").exists() {
        return Some(xcode_candidate);
    }

    None
}

fn resolve_metal_path(real_home: Option<&str>) -> Option<PathBuf> {
    resolve_toolchain_bin(real_home).map(|bin| bin.join("metal"))
}

fn resolve_metallib_path(real_home: Option<&str>) -> Option<PathBuf> {
    resolve_toolchain_bin(real_home).map(|bin| bin.join("metallib"))
}

fn compile_metal_shaders() {
    let real_home = std::env::var("HOME").ok();
    let home_override = std::env::var("METAL_HOME_OVERRIDE").unwrap_or_else(|_| {
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string())
    });
    let module_cache_dir = std::env::var("CLANG_MODULE_CACHE_PATH")
        .unwrap_or_else(|_| "target/clang-module-cache".to_string());
    // Ensure module cache directories exist to avoid sandbox permission issues
    let _ = std::fs::create_dir_all(&module_cache_dir);
    let _ = std::fs::create_dir_all(Path::new(&home_override).join(".cache/clang/ModuleCache"));

    // Rebuild if any kernel sources change
    println!("cargo:rerun-if-changed=../../metal/src/kernels/adapteros_kernels.metal");
    println!("cargo:rerun-if-changed=../../metal/src/kernels/common.metal");
    println!("cargo:rerun-if-changed=../../metal/src/kernels/attention.metal");
    println!("cargo:rerun-if-changed=../../metal/src/kernels/mlp.metal");
    println!("cargo:rerun-if-changed=../../metal/src/kernels/flash_attention.metal");
    println!("cargo:rerun-if-changed=../../metal/src/kernels/mplora.metal");
    println!("cargo:rerun-if-changed=../../metal/toolchain.toml");

    // Compile Metal kernels
    let metal_dir = Path::new("../../metal");
    let kernel_src_dir = metal_dir.join("src/kernels");
    let shaders_dir = Path::new("shaders");

    // Create shaders directory if it doesn't exist
    std::fs::create_dir_all(shaders_dir).expect("Failed to create shaders directory");

    // Verify Metal compiler is available
    let metal_check = Command::new("xcrun")
        .args(["--find", "metal"])
        .output()
        .expect("Failed to check for Metal compiler");

    if !metal_check.status.success() {
        eprintln!("\n❌ ERROR: Metal compiler not found");
        eprintln!("Install Xcode Command Line Tools: xcode-select --install");
        std::process::exit(1);
    }

    // Compile to AIR
    let compile_output = Command::new("xcrun")
        .env("CLANG_MODULE_CACHE_PATH", &module_cache_dir)
        .env("HOME", &home_override)
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-c",
            "adapteros_kernels.metal",
            "-o",
            "adapteros_kernels.air",
            "-std=metal3.1",
        ])
        .current_dir(&kernel_src_dir)
        .output()
        .expect("Failed to compile Metal shaders");

    if !compile_output.status.success() {
        let stderr = String::from_utf8_lossy(&compile_output.stderr);
        if stderr.contains("missing Metal Toolchain") {
            if let Some(metal_path) = resolve_metal_path(real_home.as_deref()) {
                let sdk_path = get_sdk_path();
                let mut fallback_cmd = Command::new(&metal_path);
                if let Some(sdk_path) = sdk_path.as_deref() {
                    fallback_cmd.arg("-isysroot").arg(sdk_path);
                }
                let fallback_output = fallback_cmd
                    .env("CLANG_MODULE_CACHE_PATH", &module_cache_dir)
                    .env("HOME", &home_override)
                    .args([
                        "-c",
                        "adapteros_kernels.metal",
                        "-o",
                        "adapteros_kernels.air",
                        "-std=metal3.1",
                    ])
                    .current_dir(&kernel_src_dir)
                    .output()
                    .expect("Failed to compile Metal shaders with fallback toolchain");
                if fallback_output.status.success() {
                    println!("cargo:warning=Using metal from {}", metal_path.display());
                } else {
                    eprintln!("\n❌ Metal compilation failed:");
                    eprintln!("{}", stderr);
                    eprintln!(
                        "Fallback metal compilation failed: {}",
                        String::from_utf8_lossy(&fallback_output.stderr)
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("\n❌ Metal compilation failed:");
                eprintln!("{}", stderr);
                eprintln!("\n🔧 SOLUTION:");
                eprintln!("Run the Metal Toolchain installer:");
                eprintln!("  ./scripts/install-metal-toolchain.sh");
                eprintln!("\nOr install manually:");
                eprintln!("  xcodebuild -downloadComponent MetalToolchain");
                eprintln!("\nOr set METAL_TOOLCHAIN_BIN to the toolchain usr/bin directory.");
                eprintln!("\nFor more information, see: docs/METAL_TOOLCHAIN_SETUP.md");
                std::process::exit(1);
            }
        } else {
            eprintln!("\n❌ Metal compilation failed:");
            eprintln!("{}", stderr);
            std::process::exit(1);
        }
    }

    // Link metallib
    let link_output = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metallib",
            "adapteros_kernels.air",
            "-o",
            "adapteros_kernels.metallib",
        ])
        .current_dir(&kernel_src_dir)
        .output()
        .expect("Failed to link metallib");

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        if stderr.contains("unable to find utility \"metallib\"") {
            if let Some(metallib_path) = resolve_metallib_path(real_home.as_deref()) {
                let fallback_output = Command::new(&metallib_path)
                    .args(["adapteros_kernels.air", "-o", "adapteros_kernels.metallib"])
                    .current_dir(&kernel_src_dir)
                    .output()
                    .expect("Failed to link metallib with fallback toolchain");
                if fallback_output.status.success() {
                    println!(
                        "cargo:warning=Using metallib from {}",
                        metallib_path.display()
                    );
                } else {
                    eprintln!(
                        "Metallib linking failed with fallback: {}",
                        String::from_utf8_lossy(&fallback_output.stderr)
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("Metallib linking failed: {}", stderr);
                eprintln!(
                    "Install the Metal Toolchain or set METAL_TOOLCHAIN_BIN to its usr/bin path."
                );
                std::process::exit(1);
            }
        } else {
            eprintln!("Metallib linking failed: {}", stderr);
            std::process::exit(1);
        }
    }

    // Verify metallib size
    let metallib_size = std::fs::metadata(kernel_src_dir.join("adapteros_kernels.metallib"))
        .expect("Failed to read metallib metadata")
        .len();

    if metallib_size == 0 {
        eprintln!("❌ Error: Generated metallib is 0 bytes!");
        std::process::exit(1);
    }

    // Compute BLAKE3 hash
    let metallib_bytes = std::fs::read(kernel_src_dir.join("adapteros_kernels.metallib"))
        .expect("Failed to read metallib");
    let hash = blake3::hash(&metallib_bytes);
    let hash_hex = hash.to_hex();

    // Write hash to output for verification
    println!("cargo:warning=Kernel hash: {}", hash_hex);
    std::fs::write(shaders_dir.join("kernel_hash.txt"), hash_hex.as_str())
        .expect("Failed to write hash");

    // Copy metallib to shaders directory
    std::fs::copy(
        kernel_src_dir.join("adapteros_kernels.metallib"),
        shaders_dir.join("adapteros_kernels.metallib"),
    )
    .expect("Failed to copy metallib");

    // Compile aos_kernels.metal from metal/ root directory
    compile_additional_kernel(
        metal_dir,
        "aos_kernels.metal",
        shaders_dir,
        &module_cache_dir,
        &home_override,
    );

    // Note: mplora_kernels is part of adapteros_kernels, create alias
    std::fs::copy(
        shaders_dir.join("adapteros_kernels.metallib"),
        shaders_dir.join("mplora_kernels.metallib"),
    )
    .expect("Failed to copy mplora_kernels alias");

    // Record build metadata
    let xcrun_version = get_xcrun_version();
    let sdk_version = get_sdk_version();
    let metadata = format!(
        r#"{{
  "xcrun_version": "{}",
  "sdk_version": "{}",
  "kernel_hash": "{}",
  "build_timestamp": "{}"
}}"#,
        xcrun_version,
        sdk_version,
        hash_hex,
        chrono::Utc::now().to_rfc3339()
    );
    std::fs::write(metal_dir.join("build_metadata.json"), metadata)
        .expect("Failed to write metadata");

    // Clean up intermediate files
    let _ = std::fs::remove_file(kernel_src_dir.join("adapteros_kernels.air"));
}

fn compile_additional_kernel(
    metal_dir: &Path,
    kernel_name: &str,
    shaders_dir: &Path,
    module_cache_dir: &str,
    home_override: &str,
) {
    let kernel_path = metal_dir.join(kernel_name);

    if !kernel_path.exists() {
        println!("cargo:warning=Kernel {} not found, skipping", kernel_name);
        return;
    }

    let kernel_stem = kernel_path
        .file_stem()
        .and_then(|s| s.to_str())
        .or_else(|| Path::new(kernel_name).file_stem().and_then(|s| s.to_str()))
        .map(str::to_owned);

    let kernel_stem = match kernel_stem {
        Some(stem) => stem,
        None => {
            println!(
                "cargo:warning=Could not derive kernel stem for {}, skipping",
                kernel_path.display()
            );
            return;
        }
    };

    let air_file = format!("{}.air", kernel_stem);
    let metallib_file = format!("{}.metallib", kernel_stem);

    // Compile to AIR
    let compile_output = Command::new("xcrun")
        .env("CLANG_MODULE_CACHE_PATH", module_cache_dir)
        .env("HOME", home_override)
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-c",
            kernel_name,
            "-o",
            &air_file,
            "-std=metal3.1",
        ])
        .current_dir(metal_dir)
        .output()
        .expect("Failed to compile additional Metal shader");

    if !compile_output.status.success() {
        let stderr = String::from_utf8_lossy(&compile_output.stderr);
        if stderr.contains("missing Metal Toolchain") {
            let real_home = std::env::var("HOME").ok();
            if let Some(metal_path) = resolve_metal_path(real_home.as_deref()) {
                let sdk_path = get_sdk_path();
                let mut fallback_cmd = Command::new(&metal_path);
                if let Some(sdk_path) = sdk_path.as_deref() {
                    fallback_cmd.arg("-isysroot").arg(sdk_path);
                }
                let fallback_output = fallback_cmd
                    .env("CLANG_MODULE_CACHE_PATH", module_cache_dir)
                    .env("HOME", home_override)
                    .args(["-c", kernel_name, "-o", &air_file, "-std=metal3.1"])
                    .current_dir(metal_dir)
                    .output()
                    .expect("Failed to compile additional Metal shader with fallback toolchain");
                if fallback_output.status.success() {
                    println!(
                        "cargo:warning=Using metal from {} for {}",
                        metal_path.display(),
                        kernel_name
                    );
                } else {
                    eprintln!("\n❌ Metal compilation failed for {}:", kernel_name);
                    eprintln!("{}", stderr);
                    eprintln!(
                        "Fallback metal compilation failed for {}: {}",
                        kernel_name,
                        String::from_utf8_lossy(&fallback_output.stderr)
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("\n❌ Metal compilation failed for {}:", kernel_name);
                eprintln!("{}", stderr);
                eprintln!(
                    "Install the Metal Toolchain or set METAL_TOOLCHAIN_BIN to its usr/bin path."
                );
                std::process::exit(1);
            }
        } else {
            eprintln!("\n❌ Metal compilation failed for {}:", kernel_name);
            eprintln!("{}", stderr);
            std::process::exit(1);
        }
    }

    // Link metallib
    let link_output = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metallib",
            &air_file,
            "-o",
            &metallib_file,
        ])
        .current_dir(metal_dir)
        .output()
        .expect("Failed to link additional metallib");

    if !link_output.status.success() {
        let stderr = String::from_utf8_lossy(&link_output.stderr);
        if stderr.contains("unable to find utility \"metallib\"") {
            let real_home = std::env::var("HOME").ok();
            if let Some(metallib_path) = resolve_metallib_path(real_home.as_deref()) {
                let fallback_output = Command::new(&metallib_path)
                    .args([&air_file, "-o", &metallib_file])
                    .current_dir(metal_dir)
                    .output()
                    .expect("Failed to link additional metallib with fallback toolchain");
                if fallback_output.status.success() {
                    println!(
                        "cargo:warning=Using metallib from {} for {}",
                        metallib_path.display(),
                        kernel_name
                    );
                } else {
                    eprintln!(
                        "Metallib linking failed for {} with fallback: {}",
                        kernel_name,
                        String::from_utf8_lossy(&fallback_output.stderr)
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("Metallib linking failed for {}: {}", kernel_name, stderr);
                eprintln!(
                    "Install the Metal Toolchain or set METAL_TOOLCHAIN_BIN to its usr/bin path."
                );
                std::process::exit(1);
            }
        } else {
            eprintln!("Metallib linking failed for {}: {}", kernel_name, stderr);
            std::process::exit(1);
        }
    }

    // Copy to shaders directory
    std::fs::copy(
        metal_dir.join(&metallib_file),
        shaders_dir.join(&metallib_file),
    )
    .expect("Failed to copy additional metallib");

    // Clean up intermediate files
    let _ = std::fs::remove_file(metal_dir.join(&air_file));
    let _ = std::fs::remove_file(metal_dir.join(&metallib_file));
}

fn get_xcrun_version() -> String {
    let output = Command::new("xcrun").arg("--version").output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("unknown")
            .to_string(),
        Err(_) => "unknown".to_string(),
    }
}

fn get_sdk_version() -> String {
    let output = Command::new("xcrun").args(["--show-sdk-version"]).output();

    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}

fn get_sdk_path() -> Option<String> {
    let output = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Generate and sign the kernel manifest with deterministic test keys.
///
/// This function:
/// 1. Reads the compiled metallib and computes its BLAKE3 hash
/// 2. Creates a manifest JSON with build metadata
/// 3. Signs the manifest with the deterministic test signing key
/// 4. Writes the manifest and signature files to the manifests directory
fn generate_signed_manifest() {
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};
    use serde::{Deserialize, Serialize};

    // Fixed seed for deterministic test key generation (same as in keys.rs)
    const TEST_KEY_SEED: [u8; 32] = [
        0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1, 0xf2, 0x03, 0x14, 0x25, 0x36, 0x47, 0x58,
        0x69, 0x7a, 0x8b, 0x9c, 0xad, 0xbe, 0xcf, 0xd0, 0xe1, 0xf2, 0x03, 0x14, 0x25, 0x36, 0x47,
        0x58, 0x69,
    ];

    #[derive(Serialize, Deserialize)]
    struct ToolchainMetadata {
        xcode_version: String,
        sdk_version: String,
        rust_version: String,
        metal_version: String,
    }

    #[derive(Serialize, Deserialize)]
    struct KernelManifest {
        kernel_hash: String,
        xcrun_version: String,
        sdk_version: String,
        rust_version: String,
        build_timestamp: String,
        toolchain_metadata: ToolchainMetadata,
    }

    #[derive(Serialize)]
    struct ManifestSignature {
        signature: String,
        public_key: String,
        algorithm: String,
        canonical_json: String,
    }

    // Read the metallib and compute its hash
    let shaders_dir = Path::new("shaders");
    let metallib_path = shaders_dir.join("adapteros_kernels.metallib");

    // Check if metallib exists (might be a fresh build)
    if !metallib_path.exists() {
        println!("cargo:warning=Metallib not found, skipping manifest signing");
        return;
    }

    let metallib_bytes = match std::fs::read(&metallib_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            println!("cargo:warning=Failed to read metallib: {}", e);
            return;
        }
    };

    let kernel_hash = blake3::hash(&metallib_bytes);
    let kernel_hash_hex = kernel_hash.to_hex();

    let manifests_dir = Path::new("manifests");
    let manifest_path = manifests_dir.join("metallib_manifest.json");
    let signature_path = manifests_dir.join("metallib_manifest.json.sig");

    let should_regenerate = if !manifest_path.exists() || !signature_path.exists() {
        true
    } else {
        match std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|json| serde_json::from_str::<KernelManifest>(&json).ok())
        {
            Some(existing) => existing.kernel_hash != kernel_hash_hex.as_str(),
            None => true,
        }
    };

    if !should_regenerate {
        return;
    }

    // Get build metadata
    let xcrun_version = get_xcrun_version();
    let sdk_version = get_sdk_version();
    let rust_version =
        std::env::var("CARGO_PKG_RUST_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let build_timestamp = {
        use chrono::TimeZone;

        std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|epoch| epoch.parse::<i64>().ok())
            .and_then(|epoch| chrono::Utc.timestamp_opt(epoch, 0).single())
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
    };

    // Create manifest
    // NOTE: kernel_hash is stored as raw hex without prefix for compatibility with B3Hash::from_hex()
    let manifest = KernelManifest {
        kernel_hash: kernel_hash_hex.to_string(),
        xcrun_version: xcrun_version.clone(),
        sdk_version: sdk_version.clone(),
        rust_version: rust_version.clone(),
        build_timestamp: build_timestamp.clone(),
        toolchain_metadata: ToolchainMetadata {
            xcode_version: xcrun_version,
            sdk_version,
            rust_version,
            metal_version: "3.1".to_string(),
        },
    };

    // Create canonical JSON (sorted keys for deterministic signing)
    let canonical_json = serde_json::to_string(&manifest).expect("Failed to serialize manifest");

    // Generate signing key from fixed seed
    let signing_key = SigningKey::from_bytes(&TEST_KEY_SEED);
    let public_key = signing_key.verifying_key();

    // Sign the canonical JSON
    let signature = signing_key.sign(canonical_json.as_bytes());

    // Create signature metadata
    let signature_metadata = ManifestSignature {
        signature: base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()),
        public_key: base64::engine::general_purpose::STANDARD.encode(public_key.to_bytes()),
        algorithm: "Ed25519".to_string(),
        canonical_json: canonical_json.clone(),
    };

    // Write manifest file
    let manifest_pretty =
        serde_json::to_string_pretty(&manifest).expect("Failed to serialize manifest");
    write_bytes_if_changed(&manifest_path, manifest_pretty.as_bytes());

    // Write signature file
    let signature_json =
        serde_json::to_string_pretty(&signature_metadata).expect("Failed to serialize signature");
    write_bytes_if_changed(&signature_path, signature_json.as_bytes());

    println!(
        "cargo:warning=Generated signed manifest with hash: {}",
        kernel_hash_hex
    );
}
