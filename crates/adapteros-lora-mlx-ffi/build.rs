// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX

use std::env;
use std::path::Path;

fn main() {
    // Tell cargo to re-run this build script if wrapper files change
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=src/mlx_cpp_wrapper.cpp");

    // Get the MLX installation path
    let mlx_path = env::var("MLX_PATH").unwrap_or_else(|_| "/opt/homebrew".to_string()); // Default to Homebrew path

    let include_dir = Path::new(&mlx_path).join("include");
    let lib_dir = Path::new(&mlx_path).join("lib");

    // Check if real MLX feature is enabled
    let real_mlx_enabled = env::var("CARGO_FEATURE_REAL_MLX").is_ok();

    // Don't link libraries for test builds to avoid linking issues
    if env::var("CARGO_CFG_TEST").is_err() {
        if real_mlx_enabled && !should_use_stub(&include_dir) {
            println!("cargo:warning=Compiling with real MLX support");
            compile_real_wrapper(&include_dir, &lib_dir);
            println!("cargo:rustc-link-lib=static=mlx_wrapper");
            println!("cargo:rustc-cfg=mlx_real");
        } else {
            if real_mlx_enabled {
                println!("cargo:warning=Real MLX feature enabled but headers not found - using stub implementation");
            }
            println!("cargo:warning=Using stub MLX implementation");
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
    // Skip bindgen entirely - we use manual FFI declarations
}

fn should_use_stub(include_dir: &Path) -> bool {
    if env::var("MLX_FORCE_STUB").is_ok() {
        return true;
    }

    // Heuristic: require MLX headers to exist
    let candidates = [
        include_dir.join("mlx.h"),
        include_dir.join("mlx/mlx.h"),
        include_dir.join("mlx/array.h"),
    ];

    !candidates.iter().any(|path| path.exists())
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
    let _include = include_dir.display().to_string();
    let lib = lib_dir.display().to_string();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("src/mlx_cpp_wrapper.cpp")
        .include(include_dir)
        .include(".");

    if cfg!(target_env = "msvc") {
        build.flag("/EHsc");
    } else {
        build.flag_if_supported("-fPIC");
        build.flag_if_supported("-O3");
        build.flag_if_supported("-Wall");
        build.flag_if_supported("-Wextra");
    }

    build.compile("mlx_wrapper");

    println!("cargo:rustc-link-search=native={}", lib);
    println!("cargo:rustc-link-lib=mlx");

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");
    } else if cfg!(target_env = "msvc") {
        // MSVC automatically links the STL
    } else {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
