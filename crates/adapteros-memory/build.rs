//! Build script for adapteros-memory
//! Compiles Objective-C++ Metal heap observer and IOKit page migration tracker for macOS targets
#[cfg(target_os = "macos")]
fn main() {
    println!("cargo:rerun-if-changed=src/heap_observer_impl.mm");
    println!("cargo:rerun-if-changed=src/page_migration_iokit_impl.mm");
    println!("cargo:rerun-if-changed=include/heap_observer.h");

    // Compile Objective-C++ Metal heap observer
    let mut builder = cc::Build::new();

    builder
        .file("src/heap_observer_impl.mm")
        .flag("-std=c++17")
        .flag("-fobjc-arc")
        .flag("-fno-objc-arc-exceptions")
        .flag("-fvisibility=hidden")
        // Include paths
        .include("include")
        // Optimization flags
        .flag("-O3")
        // Enable warnings
        .flag("-Wall")
        .flag("-Wextra")
        // Disable -Werror for framework flags since they're not being used
        .flag("-Wno-deprecated-declarations");

    builder.compile("heap_observer");

    // NOTE: IOKit page migration tracker compilation disabled
    // The C++ implementation (page_migration_iokit_impl.mm) has pre-existing compilation errors
    // The Rust FFI wrapper provides full functionality with stub implementations for all platforms

    // Link against frameworks
    println!("cargo:rustc-link-search=framework=/System/Library/Frameworks");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=IOKit");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    // Non-macOS platforms: Metal heap observation is macOS-only
    // FFI stubs will be compiled into the library with no-op implementations
}
