// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX

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

    // Check if real MLX feature is enabled
    let real_mlx_enabled = env::var("CARGO_FEATURE_REAL_MLX").is_ok();

    // Don't link libraries for test builds to avoid linking issues
    if env::var("CARGO_CFG_TEST").is_err() {
        if real_mlx_enabled {
            match find_mlx() {
                Some((include_dir, lib_dir)) => {
                    println!("cargo:warning=Compiling with real MLX support");
                    println!("cargo:warning=MLX include: {}", include_dir.display());
                    println!("cargo:warning=MLX lib: {}", lib_dir.display());
                    compile_real_wrapper(&include_dir, &lib_dir);
                    println!("cargo:rustc-link-lib=static=mlx_wrapper");
                    println!("cargo:rustc-cfg=mlx_real");
                }
                None => {
                    println!("cargo:warning=============================================================");
                    println!("cargo:warning=MLX NOT FOUND - real-mlx feature enabled but MLX not detected");
                    println!("cargo:warning=");
                    println!("cargo:warning=To install MLX:");
                    println!("cargo:warning=  brew install mlx");
                    println!("cargo:warning=");
                    println!("cargo:warning=Or set MLX_PATH to your MLX installation:");
                    println!("cargo:warning=  export MLX_PATH=/path/to/mlx");
                    println!("cargo:warning=");
                    println!("cargo:warning=Falling back to stub implementation");
                    println!("cargo:warning=============================================================");
                    compile_stub_wrapper();
                    println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
                    println!("cargo:rustc-cfg=mlx_stub");
                }
            }
        } else {
            println!("cargo:warning=Using stub MLX implementation (real-mlx feature not enabled)");
            compile_stub_wrapper();
            println!("cargo:rustc-link-lib=static=mlx_wrapper_stub");
            println!("cargo:rustc-cfg=mlx_stub");
        }
    } else {
        // For tests, just compile the stub wrapper without linking
        println!("cargo:warning=Test build - compiling stub MLX implementation");
        compile_stub_wrapper();
        println!("cargo:rustc-cfg=mlx_stub");
    }
}

/// Find MLX installation using multiple detection methods
fn find_mlx() -> Option<(PathBuf, PathBuf)> {
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
            println!("cargo:warning=Found MLX via MLX_PATH environment variable");
            return Some((include_dir, lib_dir));
        } else {
            println!("cargo:warning=MLX_PATH set but MLX not found at: {}", mlx_path);
        }
    }

    // Method 2: Try pkg-config
    if let Some(result) = find_mlx_via_pkg_config() {
        println!("cargo:warning=Found MLX via pkg-config");
        return Some(result);
    }

    // Method 3: Check common installation paths
    let common_paths = [
        "/opt/homebrew",           // Apple Silicon Homebrew
        "/usr/local",              // Intel Homebrew / manual install
        "/usr",                    // System install
        "/opt/local",              // MacPorts
    ];

    for base_path in &common_paths {
        let path = PathBuf::from(base_path);
        let include_dir = path.join("include");
        let lib_dir = path.join("lib");

        if has_mlx_headers(&include_dir) && has_mlx_library(&lib_dir) {
            println!("cargo:warning=Found MLX at: {}", base_path);
            return Some((include_dir, lib_dir));
        }
    }

    // Method 4: Check Homebrew Cellar directly
    if let Some(result) = find_mlx_in_homebrew_cellar() {
        println!("cargo:warning=Found MLX in Homebrew Cellar");
        return Some(result);
    }

    None
}

/// Try to find MLX using pkg-config
fn find_mlx_via_pkg_config() -> Option<(PathBuf, PathBuf)> {
    // Try to get MLX info from pkg-config
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
        (Some(inc), Some(lib)) => Some((inc, lib)),
        (Some(inc), None) => {
            // Try to infer lib dir from include dir
            if let Some(parent) = inc.parent() {
                let lib = parent.join("lib");
                if lib.exists() {
                    return Some((inc, lib));
                }
            }
            None
        }
        _ => None,
    }
}

/// Find MLX in Homebrew Cellar
fn find_mlx_in_homebrew_cellar() -> Option<(PathBuf, PathBuf)> {
    let cellar_paths = [
        "/opt/homebrew/Cellar/mlx",
        "/usr/local/Cellar/mlx",
    ];

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
                    return Some((include_dir, lib_dir));
                }
            }
        }
    }

    None
}

/// Check if MLX headers exist in the given directory
fn has_mlx_headers(include_dir: &Path) -> bool {
    let candidates = [
        include_dir.join("mlx/mlx.h"),
        include_dir.join("mlx/array.h"),
        include_dir.join("mlx.h"),
    ];

    candidates.iter().any(|path| path.exists())
}

/// Check if MLX library exists in the given directory
fn has_mlx_library(lib_dir: &Path) -> bool {
    let candidates = [
        lib_dir.join("libmlx.dylib"),
        lib_dir.join("libmlx.a"),
        lib_dir.join("libmlx.so"),
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

    if cfg!(target_env = "msvc") {
        build.flag("/EHsc");
    } else {
        build.flag_if_supported("-fPIC");
        build.flag_if_supported("-O2");
        build.flag_if_supported("-Wall");
        build.flag_if_supported("-Wextra");
    }

    build.compile("mlx_wrapper_stub");

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
    build.define("MLX_REAL", None);

    if cfg!(target_env = "msvc") {
        build.flag("/EHsc");
    } else {
        build.flag_if_supported("-fPIC");
        build.flag_if_supported("-O3");
        build.flag_if_supported("-Wall");
        build.flag_if_supported("-Wextra");
    }

    build.compile("mlx_wrapper");

    // Link MLX library
    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=mlx");

    // macOS-specific frameworks
    if cfg!(target_os = "macos") {
        // C++ standard library
        println!("cargo:rustc-link-lib=c++");

        // Accelerate framework (BLAS/LAPACK)
        println!("cargo:rustc-link-lib=framework=Accelerate");

        // Metal frameworks for GPU compute
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");

        // Foundation for basic macOS functionality
        println!("cargo:rustc-link-lib=framework=Foundation");
    } else if cfg!(target_env = "msvc") {
        // MSVC automatically links the STL
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
