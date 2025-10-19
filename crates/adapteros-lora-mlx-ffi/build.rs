// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    // Tell cargo to re-run this build script if wrapper files change
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=src/mlx_cpp_wrapper.cpp");
    println!("cargo:rerun-if-env-changed=MLX_PATH");
    println!("cargo:rerun-if-env-changed=MLX_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=MLX_LIB_DIR");
    println!("cargo:rerun-if-env-changed=MLX_FORCE_STUB");

    // Declare custom cfgs to the compiler so `unexpected_cfgs` lint is happy
    println!("cargo:rustc-check-cfg=cfg(mlx_stub)");
    println!("cargo:rustc-check-cfg=cfg(mlx_real)");

    // Determine include/lib dirs with env precedence:
    // 1) MLX_INCLUDE_DIR / MLX_LIB_DIR
    // 2) MLX_PATH/{include,lib}
    // 3) Default Homebrew path /opt/homebrew/{include,lib}
    let (include_dir, lib_dir, source_note) = determine_mlx_paths();

    let use_stub = should_use_stub(&include_dir);

    if use_stub {
        println!("cargo:rustc-cfg=mlx_stub");
        println!("cargo:warning=MLX FFI build: STUB");
        println!(
            "cargo:warning=Reason: MLX headers not found at: {} (source: {})",
            include_dir.display(),
            source_note
        );
        println!(
            "cargo:warning=Hint: set MLX_INCLUDE_DIR and MLX_LIB_DIR, or MLX_PATH, or export MLX_FORCE_STUB=1 to force stub"
        );

        compile_stub_wrapper();
        generate_stub_bindings();
    } else {
        println!("cargo:rustc-cfg=mlx_real");
        println!("cargo:warning=MLX FFI build: REAL");
        println!(
            "cargo:warning=Using includes: {} | libs: {} (source: {})",
            include_dir.display(),
            lib_dir.display(),
            source_note
        );

        compile_real_wrapper(&include_dir, &lib_dir);
        generate_real_bindings(&include_dir);
    }
}

fn determine_mlx_paths() -> (PathBuf, PathBuf, String) {
    // Highest precedence explicit dirs
    let inc_env = env::var("MLX_INCLUDE_DIR").ok();
    let lib_env = env::var("MLX_LIB_DIR").ok();
    if let (Some(inc), Some(lib)) = (inc_env.clone(), lib_env.clone()) {
        return (
            PathBuf::from(inc),
            PathBuf::from(lib),
            "MLX_INCLUDE_DIR/MLX_LIB_DIR".into(),
        );
    }

    // Next precedence: MLX_PATH
    let mlx_path = env::var("MLX_PATH").ok();
    if let Some(base) = mlx_path.clone() {
        let include_dir = Path::new(&base).join("include");
        let lib_dir = Path::new(&base).join("lib");
        return (include_dir, lib_dir, "MLX_PATH".into());
    }

    // Fallback: Homebrew default on Apple Silicon
    (
        PathBuf::from("/opt/homebrew/include"),
        PathBuf::from("/opt/homebrew/lib"),
        "default:/opt/homebrew".into(),
    )
}

fn should_use_stub(include_dir: &Path) -> bool {
    if env::var("MLX_FORCE_STUB")
        .ok()
        .filter(|v| v != "0")
        .is_some()
    {
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
        .include(".")
        .define("MLX_HAVE_REAL_API", None);

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

fn generate_real_bindings(include_dir: &Path) {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let include = include_dir.display().to_string();

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include))
        .clang_arg("-I.")
        .clang_arg("-DMLX_HAVE_REAL_API")
        // Suppress warnings from system headers that we can't control
        .clang_arg("-Wno-non-camel-case-types")
        .clang_arg("-Wno-non-upper-case-globals")
        .clang_arg("-Wno-non-snake-case")
        // Only generate bindings for our wrapper types and functions
        .allowlist_type("mlx_.*")
        .allowlist_function("mlx_.*")
        // Allow system types that are explicitly used in our wrapper
        .allowlist_type("wchar_t")
        .allowlist_type("max_align_t")
        .allowlist_type("int_least.*_t")
        .allowlist_type("uint_least.*_t")
        .allowlist_type("int_fast.*_t")
        .allowlist_type("uint_fast.*_t")
        .allowlist_type("__int.*_t")
        .allowlist_type("__uint.*_t")
        .allowlist_type("__darwin_.*_t")
        .allowlist_type("intmax_t")
        .allowlist_type("uintmax_t")
        .allowlist_type("__builtin_va_list")
        // Allow system constants that are needed
        .allowlist_var("__darwin_.*")
        .allowlist_var("__bool_true_false_are_defined")
        .allowlist_var("true_")
        .allowlist_var("false_")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn generate_stub_bindings() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let stub_bindings = r#"// Stub bindings for MLX FFI development
// Generated when MLX is not installed

use std::os::raw::{c_char, c_int, c_uint, c_float, c_void};

#[repr(C)]
pub struct mlx_array_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct mlx_model_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct mlx_context_t {
    _private: [u8; 0],
}

extern "C" {
    pub fn mlx_context_new() -> *mut mlx_context_t;
    pub fn mlx_context_free(ctx: *mut mlx_context_t);
    pub fn mlx_set_default_context(ctx: *mut mlx_context_t);
    
    pub fn mlx_array_from_data(data: *const c_float, size: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_from_ints(data: *const c_int, size: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_from_uints(data: *const c_uint, size: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_zeros(size: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_ones(size: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_full(size: c_int, value: c_float) -> *mut mlx_array_t;
    
    pub fn mlx_array_data(array: *mut mlx_array_t) -> *mut c_float;
    pub fn mlx_array_size(array: *mut mlx_array_t) -> c_int;
    pub fn mlx_array_shape(array: *mut mlx_array_t, shape: *mut c_int, max_dims: c_int) -> c_int;
    pub fn mlx_array_ndim(array: *mut mlx_array_t) -> c_int;
    pub fn mlx_array_dtype(array: *mut mlx_array_t) -> c_int;
    
    pub fn mlx_array_copy(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_array_reshape(array: *mut mlx_array_t, shape: *const c_int, ndim: c_int) -> *mut mlx_array_t;
    pub fn mlx_array_transpose(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_array_free(array: *mut mlx_array_t);
    
    pub fn mlx_model_load(path: *const c_char) -> *mut mlx_model_t;
    pub fn mlx_model_forward(model: *mut mlx_model_t, input: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_model_forward_with_hidden_states(
        model: *mut mlx_model_t, 
        input: *mut mlx_array_t, 
        hidden_states: *mut *mut mlx_array_t, 
        num_hidden: *mut c_int
    ) -> *mut mlx_array_t;
    pub fn mlx_free_hidden_states(hidden_states: *mut *mut mlx_array_t, num_hidden: c_int);
    pub fn mlx_model_free(model: *mut mlx_model_t);
    
    pub fn mlx_add(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_subtract(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_multiply(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_divide(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_matmul(a: *mut mlx_array_t, b: *mut mlx_array_t) -> *mut mlx_array_t;
    
    pub fn mlx_relu(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_gelu(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_sigmoid(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_tanh(array: *mut mlx_array_t) -> *mut mlx_array_t;
    pub fn mlx_softmax(array: *mut mlx_array_t) -> *mut mlx_array_t;
    
    pub fn mlx_lora_forward(
        input: *mut mlx_array_t, 
        lora_a: *mut mlx_array_t, 
        lora_b: *mut mlx_array_t, 
        alpha: c_float, 
        rank: c_float
    ) -> *mut mlx_array_t;
    pub fn mlx_lora_combine(
        base_output: *mut mlx_array_t, 
        lora_output: *mut mlx_array_t, 
        gate: c_float
    ) -> *mut mlx_array_t;
    
    pub fn mlx_get_last_error() -> *const c_char;
    pub fn mlx_clear_error();
    
    pub fn mlx_gc_collect();
    pub fn mlx_memory_usage() -> usize;

    // Build-mode probe: returns 1 if compiled with real MLX API, 0 if stub
    pub fn mlx_wrapper_is_real() -> c_int;
}
"#;

    std::fs::write(out_path.join("bindings.rs"), stub_bindings)
        .expect("Couldn't write stub bindings!");
}
