use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../metal/aos_kernels.metal");
    println!("cargo:rerun-if-changed=../../metal/common.metal");
    println!("cargo:rerun-if-changed=../../metal/fused_attention.metal");
    println!("cargo:rerun-if-changed=../../metal/fused_mlp.metal");
    println!("cargo:rerun-if-changed=../../metal/toolchain.toml");

    // Compile Metal kernels
    let metal_dir = Path::new("../../metal");
    let shaders_dir = Path::new("shaders");

    // Create shaders directory if it doesn't exist
    std::fs::create_dir_all(shaders_dir).expect("Failed to create shaders directory");

    let output = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-c",
            "aos_kernels.metal",
            "-o",
            "aos_kernels.air",
            "-std=metal3.1",
        ])
        .current_dir(metal_dir)
        .output()
        .expect("Failed to compile Metal shaders");

    if !output.status.success() {
        eprintln!("Metal compilation failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        panic!("Metal compilation failed");
    }

    // Link metallib
    let output = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metallib",
            "aos_kernels.air",
            "-o",
            "aos_kernels.metallib",
        ])
        .current_dir(metal_dir)
        .output()
        .expect("Failed to link metallib");

    if !output.status.success() {
        eprintln!("Metallib linking failed:");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        panic!("Metallib linking failed");
    }

    // Compute BLAKE3 hash
    let metallib_bytes =
        std::fs::read(metal_dir.join("aos_kernels.metallib")).expect("Failed to read metallib");
    let hash = blake3::hash(&metallib_bytes);
    let hash_hex = hash.to_hex();

    // Write hash to output for verification
    println!("cargo:warning=Kernel hash: {}", hash_hex);
    std::fs::write(shaders_dir.join("kernel_hash.txt"), hash_hex.as_str())
        .expect("Failed to write hash");

    // Copy metallib to shaders directory
    std::fs::copy(
        metal_dir.join("aos_kernels.metallib"),
        shaders_dir.join("aos_kernels.metallib"),
    )
    .expect("Failed to copy metallib");

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
    let _ = std::fs::remove_file(metal_dir.join("aos_kernels.air"));
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
