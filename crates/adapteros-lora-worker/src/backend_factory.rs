// ... existing code ...

/// Backend capability detection and reporting
pub mod capabilities {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum BackendType {
        Cpu,           // Always available
        MemoryManaged, // Basic memory tracking
        StubMLX,       // MLX stubs (no real MLX)
        StubCoreML,    // CoreML stubs (no real CoreML)
        StubMetal,     // Metal stubs (no real Metal)
        // Real backends (currently disabled)
        // RealCoreML, RealMetal, RealMLX
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackendCapability {
        pub backend_type: BackendType,
        pub name: String,
        pub available: bool,
        pub stub_only: bool,
        pub description: String,
        pub limitations: Vec<String>,
    }

    /// Get all available backend capabilities
    pub fn get_available_backends() -> Vec<BackendCapability> {
        vec![
            BackendCapability {
                backend_type: BackendType::Cpu,
                name: "CPU".to_string(),
                available: true,
                stub_only: false,
                description: "Basic CPU inference with memory management".to_string(),
                limitations: vec![],
            },
            BackendCapability {
                backend_type: BackendType::MemoryManaged,
                name: "Memory Managed".to_string(),
                available: true,
                stub_only: false,
                description: "CPU inference with comprehensive memory tracking".to_string(),
                limitations: vec![],
            },
            BackendCapability {
                backend_type: BackendType::StubMLX,
                name: "MLX (Stub)".to_string(),
                available: cfg!(feature = "mlx-backend"),
                stub_only: true,
                description: "MLX backend with stub fallback - generates dummy outputs".to_string(),
                limitations: vec![
                    "No real MLX library integration".to_string(),
                    "Generates statistically-plausible dummy logits".to_string(),
                    "No actual GPU acceleration".to_string(),
                ],
            },
            BackendCapability {
                backend_type: BackendType::StubCoreML,
                name: "CoreML (Stub)".to_string(),
                available: cfg!(feature = "coreml-backend"),
                stub_only: true,
                description: "CoreML backend - FFI layer not implemented".to_string(),
                limitations: vec![
                    "Calls non-existent FFI functions".to_string(),
                    "No real CoreML.framework integration".to_string(),
                    "No Neural Engine acceleration".to_string(),
                ],
            },
            BackendCapability {
                backend_type: BackendType::StubMetal,
                name: "Metal (Stub)".to_string(),
                available: cfg!(feature = "metal-backend"),
                stub_only: true,
                description: "Metal backend - shaders not implemented".to_string(),
                limitations: vec![
                    "No Metal Performance Shaders".to_string(),
                    "No GPU kernel execution".to_string(),
                    "No hardware acceleration".to_string(),
                ],
            },
        ]
    }

    /// Print backend status report
    pub fn print_backend_status() {
        println!("🔧 AdapterOS Backend Status Report");
        println!("===================================");
        println!();

        let backends = get_available_backends();
        let real_backends = backends.iter().filter(|b| b.available && !b.stub_only).count();
        let stub_backends = backends.iter().filter(|b| b.available && b.stub_only).count();
        let unavailable_backends = backends.iter().filter(|b| !b.available).count();

        println!("📊 Summary:");
        println!("  ✅ Real backends available: {}", real_backends);
        println!("  ⚠️  Stub backends available: {}", stub_backends);
        println!("  ❌ Backends not available: {}", unavailable_backends);
        println!();

        println!("📋 Backend Details:");
        for backend in backends {
            let status = if backend.available {
                if backend.stub_only { "⚠️  STUB" } else { "✅ REAL" }
            } else {
                "❌ N/A"
            };

            println!("  {} {} - {}", status, backend.name, backend.description);
            if !backend.limitations.is_empty() {
                for limitation in &backend.limitations {
                    println!("    • {}", limitation);
                }
            }
            println!();
        }

        println!("📖 For more details, see BACKEND_STATUS.md");
        println!("🔧 To enable real backends, see implementation roadmap in BACKEND_STATUS.md");
    }
}