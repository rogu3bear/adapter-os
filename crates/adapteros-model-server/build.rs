//! Build script for compiling protobuf definitions

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the model_server.proto file
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile_protos(&["proto/model_server.proto"], &["proto"])?;

    // Rerun if proto file changes
    println!("cargo:rerun-if-changed=proto/model_server.proto");

    Ok(())
}
