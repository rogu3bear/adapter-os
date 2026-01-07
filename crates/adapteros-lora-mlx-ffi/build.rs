// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX with comprehensive detection
//
// Features:
// - Multi-method MLX detection (env var, pkg-config, common paths, Homebrew)
// - MLX version detection from headers (supports both quoted and split defines)
// - Compile+link probe to verify header/library compatibility
// - Proper Metal, Accelerate, and Foundation framework linking
// - C++17 compiler compatibility with proper flag handling
// - Clear messaging distinguishing stub vs real MLX mode

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Tell cargo to re-run this build script if wrapper files change
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=src/mlx_cpp_wrapper.cpp");
    println!("cargo:rerun-if-changed=src/mlx_cpp_wrapper_real.cpp");
    println!("cargo:rerun-if-env-changed=MLX_PATH");
    println!("cargo:rerun-if-env-changed=MLX_FORCE_STUB");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // Check if mlx-rs-backend is enabled (pure Rust, deprecated/unsupported)
    let mlx_rs_backend = env::var("CARGO_FEATURE_MLX_RS_BACKEND").is_ok();

    // Check if C++ MLX feature is enabled
    let real_mlx_enabled =
        env::var("CARGO_FEATURE_MLX").is_ok() || env::var("CARGO_FEATURE_REAL_MLX").is_ok();

    // If using pure Rust mlx-rs backend, still compile C++ stub for FFI compatibility
    // but the actual ML operations go through mlx-rs
    if mlx_rs_backend {
        println!("cargo:warning==========================================================");
        println!("cargo:warning=MLX BACKEND: mlx-rs (deprecated)");
        println!("cargo:warning=Using Rust mlx-rs bindings with C++ stub for FFI compat");
        println!("cargo:warning==========================================================");
        compile_stub_wrapper();
        println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
        println!("cargo:rustc-cfg=mlx_stub");
        return;
    }

    // C++ FFI path
    if real_mlx_enabled {
        match find_mlx_with_version() {
            Some((include_dir, lib_dir, version)) => {
                println!("cargo:warning==========================================================");
                println!("cargo:warning=MLX BACKEND: Real C++ FFI (feature 'mlx' enabled)");
                println!("cargo:warning=MLX version: {}", version);
                println!("cargo:warning=Include path: {}", include_dir.display());
                println!("cargo:warning=Library path: {}", lib_dir.display());

                // Run compile+link probe to verify header/library compatibility
                if let Err(e) = run_mlx_compatibility_probe(&include_dir, &lib_dir) {
                    println!("cargo:warning==========================================================");
                    println!("cargo:warning=MLX COMPATIBILITY CHECK FAILED");
                    println!("cargo:warning={}", e);
                    println!("cargo:warning=");
                    println!("cargo:warning=This usually means:");
                    println!("cargo:warning=  - MLX headers and libmlx.dylib are from different versions");
                    println!("cargo:warning=  - A different libmlx is being picked up from -L path order");
                    println!("cargo:warning=  - MLX was partially installed or corrupted");
                    println!("cargo:warning=");
                    println!("cargo:warning=Try: brew reinstall mlx");
                    println!("cargo:warning==========================================================");
                    panic!("MLX header/library mismatch detected. See warnings above.");
                }

                println!("cargo:warning=Compatibility probe: PASSED");
                println!("cargo:warning==========================================================");

                compile_real_wrapper(&include_dir, &lib_dir);
                println!("cargo:rustc-link-lib=static=mlx_wrapper");
                println!("cargo:rustc-cfg=mlx_real");
            }
            None => {
                println!("cargo:warning==========================================================");
                println!("cargo:warning=MLX NOT FOUND (feature 'mlx' enabled but MLX not detected)");
                println!("cargo:warning=");
                println!("cargo:warning=To install MLX:");
                println!("cargo:warning=  brew install mlx");
                println!("cargo:warning=");
                println!("cargo:warning=Or set MLX_PATH to your MLX installation:");
                println!("cargo:warning=  export MLX_PATH=/path/to/mlx");
                println!("cargo:warning=");
                println!("cargo:warning=Falling back to STUB implementation");
                println!("cargo:warning==========================================================");
                compile_stub_wrapper();
                println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
                println!("cargo:rustc-cfg=mlx_stub");
            }
        }
    } else {
        println!("cargo:warning==========================================================");
        println!("cargo:warning=MLX BACKEND: Stub (feature 'mlx' NOT enabled)");
        println!("cargo:warning=To enable real MLX, build with: --features mlx");
        println!("cargo:warning==========================================================");
        compile_stub_wrapper();
        println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
        println!("cargo:rustc-cfg=mlx_stub");
    }
}

/// Run a compile+link probe to verify MLX headers and library are compatible.
/// This catches version mismatches before they become cryptic linker errors.
fn run_mlx_compatibility_probe(include_dir: &Path, lib_dir: &Path) -> Result<(), String> {
    let out_dir = env::var("OUT_DIR").map_err(|e| format!("OUT_DIR not set: {}", e))?;
    let probe_src = PathBuf::from(&out_dir).join("mlx_probe.cpp");
    let probe_out = PathBuf::from(&out_dir).join("mlx_probe");

    // Write a minimal probe that includes MLX headers and references key symbols.
    // This verifies that headers and library are from compatible versions.
    let probe_code = r#"
// MLX compatibility probe - verifies headers and library match
#include <mlx/mlx.h>
#include <mlx/array.h>
#include <mlx/ops.h>

int main() {
    // Reference symbols we depend on to verify they exist in libmlx.
    // These are core functions that must be present for the FFI to work.

    // 1. Array creation (fundamental)
    mlx::core::array arr = mlx::core::zeros({2, 2});

    // 2. Basic operation (verifies ops linkage)
    mlx::core::array result = mlx::core::add(arr, arr);

    // 3. Synchronize to ensure compute graph is linked
    result.item<float>();

    return 0;
}
"#;

    std::fs::write(&probe_src, probe_code)
        .map_err(|e| format!("Failed to write probe source: {}", e))?;

    // Compile and link the probe using the same paths we'll use for the real build
    let compiler = env::var("CXX").unwrap_or_else(|_| "c++".to_string());

    let mut args = vec![
        "-std=c++17".to_string(),
        "-o".to_string(),
        probe_out.to_str().unwrap().to_string(),
        probe_src.to_str().unwrap().to_string(),
        format!("-I{}", include_dir.display()),
        format!("-L{}", lib_dir.display()),
        "-lmlx".to_string(),
        "-lc++".to_string(),
    ];

    // Add framework flags on macOS
    if cfg!(target_os = "macos") {
        args.extend([
            "-framework".to_string(),
            "Metal".to_string(),
            "-framework".to_string(),
            "Foundation".to_string(),
            "-framework".to_string(),
            "Accelerate".to_string(),
        ]);
    }

    let output = Command::new(&compiler)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run compiler for probe: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Clean up probe files
        let _ = std::fs::remove_file(&probe_src);

        return Err(format!(
            "Probe compilation failed.\n\
             Include dir: {}\n\
             Library dir: {}\n\
             Compiler: {}\n\
             Stdout: {}\n\
             Stderr: {}",
            include_dir.display(),
            lib_dir.display(),
            compiler,
            stdout,
            stderr
        ));
    }

    // Clean up probe files
    let _ = std::fs::remove_file(&probe_src);
    let _ = std::fs::remove_file(&probe_out);

    Ok(())
}

/// Find MLX installation with version detection using multiple detection methods
fn find_mlx_with_version() -> Option<(PathBuf, PathBuf, String)> {
    // Check for forced stub
    if env::var("MLX_FORCE_STUB").is_ok() {
        println!("cargo:warning=MLX_FORCE_STUB set - using stub implementation");
        return None;
    }

    // Method 1: Check MLX_PATH environment variable
    if let Ok(mlx_path) = env::var("MLX_PATH") {
        let path = PathBuf::from(&mlx_path);
        let include_dir = path.join("include");
        let lib_dir = path.join("lib");

        if has_mlx_headers(&include_dir) && has_mlx_library(&lib_dir) {
            let version = detect_mlx_version(&include_dir);
            println!(
                "cargo:warning=Found MLX via MLX_PATH: {} ({})",
                mlx_path, version
            );
            return Some((include_dir, lib_dir, version));
        } else {
            println!(
                "cargo:warning=MLX_PATH set but MLX not found at: {}",
                mlx_path
            );
        }
    }

    // Method 2: Try pkg-config
    if let Some((include_dir, lib_dir, version)) = find_mlx_via_pkg_config() {
        println!("cargo:warning=Found MLX via pkg-config ({})", version);
        return Some((include_dir, lib_dir, version));
    }

    // Method 3: Check common installation paths
    let common_paths = [
        "/opt/homebrew", // Apple Silicon Homebrew
        "/usr/local",    // Intel Homebrew / manual install
        "/usr",          // System install
        "/opt/local",    // MacPorts
    ];

    for base_path in &common_paths {
        let path = PathBuf::from(base_path);
        let include_dir = path.join("include");
        let lib_dir = path.join("lib");

        if has_mlx_headers(&include_dir) && has_mlx_library(&lib_dir) {
            let version = detect_mlx_version(&include_dir);
            println!(
                "cargo:warning=Found MLX at: {} ({})",
                base_path, version
            );
            return Some((include_dir, lib_dir, version));
        }
    }

    // Method 4: Check Homebrew Cellar directly
    if let Some((include_dir, lib_dir, version)) = find_mlx_in_homebrew_cellar() {
        println!("cargo:warning=Found MLX in Homebrew Cellar ({})", version);
        return Some((include_dir, lib_dir, version));
    }

    None
}

/// Try to find MLX using pkg-config with version detection
fn find_mlx_via_pkg_config() -> Option<(PathBuf, PathBuf, String)> {
    // First, try to get version
    let version_output = Command::new("pkg-config")
        .args(["--modversion", "mlx"])
        .output()
        .ok()?;

    let version = if version_output.status.success() {
        String::from_utf8_lossy(&version_output.stdout)
            .trim()
            .to_string()
    } else {
        "unknown".to_string()
    };

    // Get cflags and libs
    let output = Command::new("pkg-config")
        .args(["--cflags", "--libs", "mlx"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut include_dir = None;
    let mut lib_dir = None;

    for part in stdout.split_whitespace() {
        if let Some(path) = part.strip_prefix("-I") {
            include_dir = Some(PathBuf::from(path));
        } else if let Some(path) = part.strip_prefix("-L") {
            lib_dir = Some(PathBuf::from(path));
        }
    }

    match (include_dir, lib_dir) {
        (Some(inc), Some(lib)) => Some((inc, lib, version)),
        (Some(inc), None) => {
            // Try to infer lib dir from include dir
            if let Some(parent) = inc.parent() {
                let lib = parent.join("lib");
                if lib.exists() {
                    return Some((inc, lib, version));
                }
            }
            None
        }
        _ => None,
    }
}

/// Find MLX in Homebrew Cellar with version detection
fn find_mlx_in_homebrew_cellar() -> Option<(PathBuf, PathBuf, String)> {
    let cellar_paths = ["/opt/homebrew/Cellar/mlx", "/usr/local/Cellar/mlx"];

    for cellar_path in &cellar_paths {
        let cellar = PathBuf::from(cellar_path);
        if !cellar.exists() {
            continue;
        }

        // Find the latest version directory
        if let Ok(entries) = std::fs::read_dir(&cellar) {
            let mut versions: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .collect();

            // Sort by name (version) descending
            versions.sort_by_key(|e| std::cmp::Reverse(e.file_name()));

            if let Some(version_entry) = versions.first() {
                let version_path = version_entry.path();
                let include_dir = version_path.join("include");
                let lib_dir = version_path.join("lib");

                if has_mlx_headers(&include_dir) && has_mlx_library(&lib_dir) {
                    // Use directory name as version (Homebrew convention)
                    let version = version_entry.file_name().to_string_lossy().to_string();
                    return Some((include_dir, lib_dir, version));
                }
            }
        }
    }

    None
}

/// Detect MLX version from header files.
/// Supports both old style (#define MLX_VERSION "x.y.z") and
/// new style (separate MLX_VERSION_MAJOR/MINOR/PATCH defines).
fn detect_mlx_version(include_dir: &Path) -> String {
    let version_h_paths = [
        include_dir.join("mlx/version.h"),
        include_dir.join("version.h"),
    ];

    for version_h in &version_h_paths {
        if version_h.exists() {
            if let Ok(content) = std::fs::read_to_string(version_h) {
                if let Some(version) = extract_version_from_header(&content) {
                    return version;
                }
            }
        }
    }

    // Fallback - version detection failed
    "unknown".to_string()
}

/// Extract version string from C header file content.
/// Supports two patterns:
/// 1. Old style: #define MLX_VERSION "0.15.0"
/// 2. New style: #define MLX_VERSION_MAJOR 0 / MLX_VERSION_MINOR 30 / MLX_VERSION_PATCH 1
fn extract_version_from_header(content: &str) -> Option<String> {
    // Try old style first: #define MLX_VERSION "x.y.z"
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(after_define) = trimmed.strip_prefix("#define") {
            let after_define = after_define.trim();
            // Look for MLX_VERSION followed by a quoted string (not MLX_VERSION_MAJOR etc)
            if after_define.starts_with("MLX_VERSION ")
                && !after_define.starts_with("MLX_VERSION_")
            {
                if let Some(start) = after_define.find('"') {
                    if let Some(end) = after_define[start + 1..].find('"') {
                        return Some(after_define[start + 1..start + 1 + end].to_string());
                    }
                }
            }
        }
    }

    // Try new style: separate MAJOR/MINOR/PATCH defines
    let mut major: Option<u32> = None;
    let mut minor: Option<u32> = None;
    let mut patch: Option<u32> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(after_define) = trimmed.strip_prefix("#define") {
            let after_define = after_define.trim();

            if let Some(rest) = after_define.strip_prefix("MLX_VERSION_MAJOR") {
                major = rest.trim().parse().ok();
            } else if let Some(rest) = after_define.strip_prefix("MLX_VERSION_MINOR") {
                minor = rest.trim().parse().ok();
            } else if let Some(rest) = after_define.strip_prefix("MLX_VERSION_PATCH") {
                patch = rest.trim().parse().ok();
            }
        }
    }

    // If we found all three components, format as semver
    match (major, minor, patch) {
        (Some(maj), Some(min), Some(pat)) => Some(format!("{}.{}.{}", maj, min, pat)),
        (Some(maj), Some(min), None) => Some(format!("{}.{}.0", maj, min)),
        (Some(maj), None, None) => Some(format!("{}.0.0", maj)),
        _ => None,
    }
}

/// Check if MLX headers exist in the given directory
fn has_mlx_headers(include_dir: &Path) -> bool {
    // Check for primary header locations
    let candidates = [
        include_dir.join("mlx/mlx.h"),
        include_dir.join("mlx/array.h"),
        include_dir.join("mlx/ops.h"),
        include_dir.join("mlx.h"),
    ];

    candidates.iter().any(|path| path.exists())
}

/// Check if MLX library exists in the given directory
fn has_mlx_library(lib_dir: &Path) -> bool {
    // Check for library files with different formats
    let candidates = [
        lib_dir.join("libmlx.dylib"),
        lib_dir.join("libmlx.so"),
        lib_dir.join("libmlx.a"),
        lib_dir.join("mlx.lib"),    // Windows
        lib_dir.join("libmlx.lib"), // Alternative Windows
    ];

    candidates.iter().any(|path| path.exists())
}

fn compile_stub_wrapper() {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("src/mlx_cpp_wrapper.cpp")
        .include(".");

    // Compiler-specific optimizations and error handling
    if cfg!(target_env = "msvc") {
        // MSVC: Enable exception handling and multi-threaded runtime
        build.flag("/EHsc"); // Enable C++ exceptions
        build.flag("/MD"); // Multi-threaded DLL runtime
        build.flag("/O2"); // Optimize for speed
    } else {
        // GCC/Clang: Standard optimization and warning flags
        build.flag_if_supported("-fPIC"); // Position-independent code
        build.flag_if_supported("-O2"); // Standard optimization
        build.flag_if_supported("-Wall"); // All warnings
        build.flag_if_supported("-Wextra"); // Extra warnings
        build.flag_if_supported("-fvisibility=hidden"); // Hidden symbols by default

        // C++17 standard compatibility flags for older toolchains
        build.flag_if_supported("-std=c++17");
        build.flag_if_supported("-fno-strict-aliasing"); // Safety with C bindings
    }

    build.compile("mlx_wrapper_stub");

    // Link C++ standard library
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
    } else if cfg!(target_env = "msvc") {
        // MSVC links the C++ runtime automatically
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }
}

fn compile_real_wrapper(include_dir: &Path, lib_dir: &Path) {
    let lib = lib_dir.display().to_string();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("src/mlx_cpp_wrapper_real.cpp")
        .include(include_dir)
        .include(".");

    // Define macro to indicate real MLX compilation
    build.define("MLX_REAL", "1");

    // Compiler-specific optimizations and error handling
    if cfg!(target_env = "msvc") {
        // MSVC: Enable exception handling and multi-threaded runtime
        build.flag("/EHsc"); // Enable C++ exceptions
        build.flag("/MD"); // Multi-threaded DLL runtime
        build.flag("/O3"); // Maximum optimization
        build.flag("/std:c++17"); // Explicit C++17 for MSVC
    } else {
        // GCC/Clang: Aggressive optimization and warning flags for performance-critical code
        build.flag_if_supported("-fPIC"); // Position-independent code
        build.flag_if_supported("-O3"); // Maximum optimization (performance-critical)
                                        // NOTE: -march=native is intentionally NOT used here due to Apple clang 17.0 bug
                                        // that causes "No way to correctly truncate anything but float to bfloat" error
                                        // when compiling MLX headers with bfloat16 types. The performance impact is minimal
                                        // as MLX operations run on GPU anyway.
        build.flag_if_supported("-Wall"); // All warnings
        build.flag_if_supported("-Wextra"); // Extra warnings
        build.flag_if_supported("-fvisibility=hidden"); // Hidden symbols by default

        // C++17 standard compatibility flags
        build.flag_if_supported("-std=c++17");
        build.flag_if_supported("-fno-strict-aliasing"); // Safety with C bindings
                                                         // NOTE: fast-math flags are explicitly prohibited per AGENTS.md invariant.
                                                         // They enable unsafe FP optimizations that violate IEEE 754 semantics
                                                         // and cause non-deterministic inference results across runs.
    }

    build.compile("mlx_wrapper");

    // Link MLX library
    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=mlx");

    // Platform-specific framework and library linking
    if cfg!(target_os = "macos") {
        // C++ standard library
        println!("cargo:rustc-link-lib=c++");

        // Accelerate framework (BLAS, LAPACK, vDSP, Sparse)
        println!("cargo:rustc-link-lib=framework=Accelerate");

        // Metal frameworks for GPU compute (ANE acceleration)
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");

        // Core foundation for system-level functionality
        println!("cargo:rustc-link-lib=framework=Foundation");

        // CoreML (if needed for model loading optimization)
        println!("cargo:rustc-link-lib=framework=CoreML");
    } else if cfg!(target_env = "msvc") {
        // MSVC: C++ runtime is linked automatically
        // Link Windows math libraries if needed
        println!("cargo:rustc-link-lib=Advapi32");
    } else {
        // Linux/Unix: Use standard C++ library
        println!("cargo:rustc-link-lib=stdc++");
        // May need pthread for threading support
        println!("cargo:rustc-link-lib=pthread");
    }
}
