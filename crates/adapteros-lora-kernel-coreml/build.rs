use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "macos" {
        println!("cargo:warning=Skipping CoreML compilation on non-macOS platform");
        return;
    }

    compile_coreml_bridge();
    compile_swift_bridge();
}

fn compile_coreml_bridge() {
    println!("cargo:rerun-if-changed=src/coreml_bridge.mm");
    println!("cargo:rerun-if-changed=src/coreml_ffi.h");

    let bridge_path = PathBuf::from("src/coreml_bridge.mm");

    if !bridge_path.exists() {
        panic!("CoreML bridge source not found: src/coreml_bridge.mm");
    }

    cc::Build::new()
        .file("src/coreml_bridge.mm")
        .flag("-framework")
        .flag("CoreML")
        .flag("-framework")
        .flag("Foundation")
        .flag("-framework")
        .flag("Metal")
        .flag("-std=c++17")
        .flag("-fobjc-arc")
        .flag("-fno-fast-math")
        .cpp(true)
        .compile("coreml_bridge");

    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=Accelerate");

    println!("cargo:warning=CoreML bridge compiled successfully");
}

fn compile_swift_bridge() {
    println!("cargo:rerun-if-changed=swift/CoreMLBridge.swift");

    let swift_source = PathBuf::from("swift/CoreMLBridge.swift");

    if !swift_source.exists() {
        println!("cargo:warning=Swift bridge source not found: swift/CoreMLBridge.swift, skipping");
        return;
    }

    // Check if swiftc is available
    let swiftc_check = Command::new("swiftc").arg("--version").output();
    if swiftc_check.is_err() {
        println!("cargo:warning=swiftc not found, skipping Swift bridge compilation");
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let obj_path = format!("{}/CoreMLSwiftBridge.o", out_dir);
    let lib_path = format!("{}/libCoreMLSwiftBridge.a", out_dir);
    let swift_source_str = swift_source.to_str().expect("Invalid swift source path");

    // Compile Swift to object file
    let compile_status = Command::new("swiftc")
        .args(&[
            "-c",
            "-O",
            "-emit-object",
            "-module-name",
            "CoreMLSwiftBridge",
            "-o",
            &obj_path,
            swift_source_str,
        ])
        .status()
        .expect("Failed to execute swiftc");

    if !compile_status.success() {
        panic!("Failed to compile Swift bridge");
    }

    // Create static library
    let ar_status = Command::new("ar")
        .args(&["rcs", &lib_path, &obj_path])
        .status()
        .expect("Failed to execute ar");

    if !ar_status.success() {
        panic!("Failed to create static library from Swift object");
    }

    // Link directives
    println!("cargo:rustc-link-lib=static=CoreMLSwiftBridge");
    println!("cargo:rustc-link-search=native={}", out_dir);

    // Link Swift runtime libraries
    println!("cargo:rustc-link-lib=dylib=swiftCore");

    // Find Xcode toolchain for Swift concurrency library
    let xcode_dev_path = std::process::Command::new("xcode-select")
        .arg("-p")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "/Applications/Xcode.app/Contents/Developer".to_string());

    // Add rpath for Swift libraries
    let swift_lib_path = format!(
        "{}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx",
        xcode_dev_path
    );
    println!("cargo:rustc-link-search=native={}", swift_lib_path);
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", swift_lib_path);

    println!("cargo:warning=Swift CoreML bridge compiled successfully");
}
