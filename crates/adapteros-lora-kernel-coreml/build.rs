fn main() {
    // Only build on macOS
    if !cfg!(target_os = "macos") {
        println!("cargo:warning=CoreML backend only available on macOS");
        return;
    }

    // Check for macOS 13+ (required for ANE)
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");

    // Compile Objective-C++ implementation
    cc::Build::new()
        .cpp(true)
        .file("src/coreml_backend.mm")
        .flag("-std=c++17")
        .flag("-fno-exceptions")
        .flag("-fno-fast-math") // Ensure determinism
        .flag("-fobjc-arc") // Enable ARC for CoreML (conditional determinism)
        .compile("coreml_backend");

    // Link CoreML and Foundation frameworks
    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Accelerate");

    // Link IOKit for power management (battery, thermal)
    println!("cargo:rustc-link-lib=framework=IOKit");

    // Optional: Link Metal for GPU fallback
    println!("cargo:rustc-link-lib=framework=Metal");
}
