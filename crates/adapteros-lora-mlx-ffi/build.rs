// Build script for MLX FFI crate
// Compiles C++ wrapper and links against MLX

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to re-run this build script if wrapper files change
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=src/mlx_cpp_wrapper.cpp");

    // Get the MLX installation path
    let mlx_path = env::var("MLX_PATH").unwrap_or_else(|_| "/opt/homebrew".to_string()); // Default to Homebrew path

    let _mlx_include = format!("{}/include", mlx_path);
    let _mlx_lib = format!("{}/lib", mlx_path);

    // Always use stub implementation for now
    // MLX is primarily a Python framework and doesn't have a C++ model API
    println!("cargo:warning=Using stub MLX implementation (MLX is Python-first framework)");
    println!("cargo:warning=For production use, consider integrating with MLX Python API via PyO3");

    // Generate stub bindings for development
    generate_stub_bindings();
    return;

    #[allow(unreachable_code)]
    // Compile the C++ wrapper
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("src/mlx_cpp_wrapper.cpp")
        .include(&_mlx_include)
        .include(".")
        .flag("-fPIC")
        .flag("-O3")
        .flag("-Wall")
        .flag("-Wextra")
        .compile("mlx_wrapper");

    // Link against MLX libraries
    println!("cargo:rustc-link-search=native={}", _mlx_lib);
    println!("cargo:rustc-link-lib=mlx");
    println!("cargo:rustc-link-lib=c++");

    // Tell cargo to link against system libraries
    println!("cargo:rustc-link-lib=framework=Accelerate");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=MetalKit");
    println!("cargo:rustc-link-lib=framework=MetalPerformanceShaders");

    // Set up bindgen for FFI bindings
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", _mlx_include))
        .clang_arg("-I.")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
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
}
"#;

    std::fs::write(out_path.join("bindings.rs"), stub_bindings)
        .expect("Couldn't write stub bindings!");
}
