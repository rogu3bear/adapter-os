// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX with comprehensive detection
//
// Features:
// - Multi-method MLX detection (env var, pkg-config, common paths, Homebrew)
// - MLX version detection and validation
// - Proper Metal, Accelerate, and Foundation framework linking
// - C++17 compiler compatibility with proper flag handling
// - Comprehensive include path detection with fallback strategies

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

    // Check if real MLX feature is enabled (new name `mlx`, keep legacy alias)
    let real_mlx_enabled =
        env::var("CARGO_FEATURE_MLX").is_ok() || env::var("CARGO_FEATURE_REAL_MLX").is_ok();

    // Use consistent wrapper for both lib and test builds
    if real_mlx_enabled {
        match find_mlx_with_version() {
            Some((include_dir, lib_dir, version)) => {
                println!(
                    "cargo:warning=Compiling with real MLX support (v{})",
                    version
                );
                println!("cargo:warning=MLX include: {}", include_dir.display());
                println!("cargo:warning=MLX lib: {}", lib_dir.display());
                compile_real_wrapper(&include_dir, &lib_dir);
                println!("cargo:rustc-link-lib=static=mlx_wrapper");
                println!("cargo:rustc-cfg=mlx_real");
            }
            None => {
                println!(
                    "cargo:warning============================================================="
                );
                println!("cargo:warning=MLX NOT FOUND - mlx feature enabled but MLX not detected");
                println!("cargo:warning=");
                println!("cargo:warning=To install MLX:");
                println!("cargo:warning=  brew install mlx");
                println!("cargo:warning=");
                println!("cargo:warning=Or set MLX_PATH to your MLX installation:");
                println!("cargo:warning=  export MLX_PATH=/path/to/mlx");
                println!("cargo:warning=");
                println!("cargo:warning=Falling back to stub implementation");
                println!(
                    "cargo:warning============================================================="
                );
                compile_stub_wrapper();
                println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
                println!("cargo:rustc-cfg=mlx_stub");
            }
        }
    } else {
        println!("cargo:warning=Using stub MLX implementation (mlx feature not enabled)");
        compile_stub_wrapper();
        println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
        println!("cargo:rustc-cfg=mlx_stub");
    }
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
            if let Some(version) = detect_mlx_version(&include_dir, &lib_dir) {
                println!(
                    "cargo:warning=Found MLX via MLX_PATH environment variable (v{})",
                    version
                );
                return Some((include_dir, lib_dir, version));
            }
        } else {
            println!(
                "cargo:warning=MLX_PATH set but MLX not found at: {}",
                mlx_path
            );
        }
    }

    // Method 2: Try pkg-config
    if let Some((include_dir, lib_dir, version)) = find_mlx_via_pkg_config() {
        println!("cargo:warning=Found MLX via pkg-config (v{})", version);
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
            if let Some(version) = detect_mlx_version(&include_dir, &lib_dir) {
                println!("cargo:warning=Found MLX at: {} (v{})", base_path, version);
                return Some((include_dir, lib_dir, version));
            }
        }
    }

    // Method 4: Check Homebrew Cellar directly
    if let Some((include_dir, lib_dir, version)) = find_mlx_in_homebrew_cellar() {
        println!("cargo:warning=Found MLX in Homebrew Cellar (v{})", version);
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
                    let version = version_entry.file_name().to_string_lossy().to_string();
                    return Some((include_dir, lib_dir, version));
                }
            }
        }
    }

    None
}

/// Detect MLX version from version.h or library metadata
fn detect_mlx_version(include_dir: &Path, lib_dir: &Path) -> Option<String> {
    // Method 1: Try to read version from mlx/version.h
    let version_h_paths = [
        include_dir.join("mlx/version.h"),
        include_dir.join("version.h"),
    ];

    for version_h in &version_h_paths {
        if version_h.exists() {
            if let Ok(content) = std::fs::read_to_string(version_h) {
                // Try to parse version from header file
                if let Some(version) = extract_version_from_header(&content) {
                    return Some(version);
                }
            }
        }
    }

    // Method 2: Try to extract from dylib/library file metadata (macOS)
    if cfg!(target_os = "macos") {
        let dylib_path = lib_dir.join("libmlx.dylib");
        if dylib_path.exists() {
            if let Some(version) = get_dylib_version(&dylib_path) {
                return Some(version);
            }
        }
    }

    // Method 3: Default fallback
    Some("0.0.0".to_string())
}

/// Extract version string from C header file content
fn extract_version_from_header(content: &str) -> Option<String> {
    // Look for patterns like: #define MLX_VERSION "0.15.0"
    for line in content.lines() {
        if let Some(after_define) = line.strip_prefix("#define") {
            if after_define.contains("VERSION") {
                // Extract version string from quotes
                if let Some(start) = after_define.find('"') {
                    if let Some(end) = after_define[start + 1..].find('"') {
                        return Some(after_define[start + 1..start + 1 + end].to_string());
                    }
                }
            }
        }
    }
    None
}

/// Get version from macOS dylib file using otool or other methods
fn get_dylib_version(dylib_path: &Path) -> Option<String> {
    // Try using 'otool' to extract version information
    if let Ok(output) = Command::new("otool")
        .args(["-L", dylib_path.to_str()?])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse version from library path if available
            for line in stdout.lines() {
                if let Some(version_part) = line.split('/').next_back() {
                    if version_part.contains(".dylib") {
                        // Try to extract version number from line
                        if let Some(caps) = version_part.split('.').next() {
                            if let Some(v) = caps.split('_').next_back() {
                                return Some(v.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
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

    let found = candidates.iter().any(|path| path.exists());

    if !found {
        println!(
            "cargo:warning=No MLX headers found in: {}",
            include_dir.display()
        );
        println!("cargo:warning=Checked for: mlx/mlx.h, mlx/array.h, mlx/ops.h, mlx.h");
    }

    found
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

    let found = candidates.iter().any(|path| path.exists());

    if !found {
        println!(
            "cargo:warning=No MLX library found in: {}",
            lib_dir.display()
        );
        println!(
            "cargo:warning=Checked for: libmlx.dylib, libmlx.so, libmlx.a, mlx.lib, libmlx.lib"
        );
    }

    found
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
                                                         // NOTE: -ffast-math is explicitly PROHIBITED per AGENTS.md invariant
                                                         // "No `-ffast-math` compiler flags - Breaks determinism"
                                                         // This flag enables unsafe FP optimizations that violate IEEE 754 semantics
                                                         // and cause non-deterministic inference results across runs.
    }

    build.compile("mlx_wrapper");

    // Link MLX library
    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=mlx");

    // Platform-specific framework and library linking
    if cfg!(target_os = "macos") {
        println!("cargo:warning=Linking macOS frameworks for MLX");

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
