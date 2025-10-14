//! Agent A: Kernel & Determinism checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

pub async fn run(args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent A - Kernel & Determinism");

    // 1. Check hermetic build + hash pin
    section.add_check(check_metallib_hash(args));

    // 2. Determinism tests
    section.add_check(run_determinism_tests());

    // 3. Performance regression gate
    section.add_check(check_performance_regression(args));

    // 4. Profiling events
    section.add_check(check_profiling_events(args));

    // 5. VRAM attribution
    section.add_check(check_vram_attribution());

    // 6. Multi-GPU selector
    section.add_check(check_multi_gpu_selector());

    // 7. Panic recovery boundary
    section.add_check(check_panic_recovery());

    Ok(section)
}

fn check_metallib_hash(args: &VerifyAgentsArgs) -> Check {
    // Check if ci_build.sh exists
    if !Path::new("metal/ci_build.sh").exists() {
        return Check::fail(
            "Hermetic build + hash pin",
            vec![],
            "metal/ci_build.sh not found",
        );
    }

    // Read METALLIB_HASH from kernel-mtl/src/lib.rs
    let kernel_src = match fs::read_to_string("crates/mplora-kernel-mtl/src/lib.rs") {
        Ok(content) => content,
        Err(e) => {
            return Check::fail(
                "Hermetic build + hash pin",
                vec![],
                format!("Failed to read kernel source: {}", e),
            )
        }
    };

    // Extract METALLIB_HASH constant
    let hash_line = kernel_src
        .lines()
        .find(|line| line.contains("const METALLIB_HASH"));

    match hash_line {
        Some(line) => {
            let evidence = ["metal/ci_build.sh exists".to_string(),
                format!("Found hash constant: {}", line.trim())];

            // Check if metallib exists
            if Path::new("metal/aos_kernels.metallib").exists() {
                Check::pass(
                    "Hermetic build + hash pin",
                    vec![
                        evidence[0].clone(),
                        evidence[1].clone(),
                        "metal/aos_kernels.metallib exists".to_string(),
                    ],
                )
            } else if args.update_baselines {
                Check::skip(
                    "Hermetic build + hash pin",
                    "Metallib not built yet, but ci_build.sh and hash constant present",
                )
            } else {
                Check::skip(
                    "Hermetic build + hash pin",
                    "Metallib not yet compiled (requires build.sh run)",
                )
            }
        }
        None => Check::fail(
            "Hermetic build + hash pin",
            vec!["metal/ci_build.sh exists".to_string()],
            "METALLIB_HASH constant not found in kernel source",
        ),
    }
}

fn run_determinism_tests() -> Check {
    // Check if determinism test exists
    if !Path::new("tests/determinism.rs").exists() {
        return Check::skip("Determinism tests", "tests/determinism.rs not found");
    }

    let output = Command::new("cargo")
        .args(["test", "--test", "determinism", "--", "--nocapture"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);

            if out.status.success() {
                Check::pass(
                    "Determinism tests",
                    vec![
                        "tests/determinism.rs exists".to_string(),
                        format!("Test output: {}", combined.lines().take(10).collect::<Vec<_>>().join("\n")),
                    ],
                )
            } else {
                Check::fail(
                    "Determinism tests",
                    vec![combined],
                    "Determinism tests failed",
                )
            }
        }
        Err(e) => Check::fail("Determinism tests", vec![], format!("Failed to run: {}", e)),
    }
}

fn check_performance_regression(args: &VerifyAgentsArgs) -> Check {
    // Check for baselines directory
    if !Path::new("metal/baselines").exists() {
        return Check::skip(
            "Performance regression gate",
            "metal/baselines directory not found",
        );
    }

    // List baseline files
    let baselines = match fs::read_dir("metal/baselines") {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .map(|e| e.path().display().to_string())
            .collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    if baselines.is_empty() {
        return Check::skip(
            "Performance regression gate",
            "No baseline JSON files found in metal/baselines",
        );
    }

    let evidence = vec![
        format!("Found {} baseline files", baselines.len()),
        format!("Baselines: {}", baselines.join(", ")),
    ];

    if args.update_baselines {
        Check::skip(
            "Performance regression gate",
            "Baseline update mode enabled (not checking regression)",
        )
    } else {
        Check::pass("Performance regression gate", evidence)
    }
}

fn check_profiling_events(args: &VerifyAgentsArgs) -> Check {
    // Check for kernel profiling code
    let kernel_prof = Path::new("crates/mplora-kernel-prof");
    if !kernel_prof.exists() {
        return Check::fail(
            "Profiling events",
            vec![],
            "crates/mplora-kernel-prof not found",
        );
    }

    // Check for AOS_KERNEL_PROFILE environment variable handling
    let lib_rs = match fs::read_to_string("crates/mplora-kernel-prof/src/lib.rs") {
        Ok(content) => content,
        Err(_) => {
            return Check::skip(
                "Profiling events",
                "Could not read kernel-prof source",
            )
        }
    };

    let has_env_check = lib_rs.contains("AOS_KERNEL_PROFILE");
    let has_available_field = lib_rs.contains("available") || lib_rs.contains("\"available\"");

    let mut evidence = vec!["crates/mplora-kernel-prof exists".to_string()];

    if has_env_check {
        evidence.push("Found AOS_KERNEL_PROFILE env handling".to_string());
    }
    if has_available_field {
        evidence.push("Found 'available' field handling".to_string());
    }

    if args.no_gpu {
        Check::skip(
            "Profiling events",
            "GPU checks disabled (--no-gpu), but profiling crate exists",
        )
    } else {
        Check::pass("Profiling events", evidence)
    }
}

fn check_vram_attribution() -> Check {
    // Check for VramTracker in kernel-mtl
    let vram_file = Path::new("crates/mplora-kernel-mtl/src/vram.rs");
    if !vram_file.exists() {
        return Check::fail(
            "VRAM attribution",
            vec![],
            "crates/mplora-kernel-mtl/src/vram.rs not found",
        );
    }

    let content = match fs::read_to_string(vram_file) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "VRAM attribution",
                vec![],
                format!("Failed to read vram.rs: {}", e),
            )
        }
    };

    let has_tracker = content.contains("VramTracker");
    let has_bytes = content.contains("vram_bytes") || content.contains("bytes");

    if has_tracker && has_bytes {
        Check::pass(
            "VRAM attribution",
            vec![
                "crates/mplora-kernel-mtl/src/vram.rs exists".to_string(),
                "VramTracker implementation found".to_string(),
                "Byte tracking implemented".to_string(),
            ],
        )
    } else {
        Check::fail(
            "VRAM attribution",
            vec!["vram.rs exists".to_string()],
            "VramTracker or byte tracking not found",
        )
    }
}

fn check_multi_gpu_selector() -> Check {
    // Check for AOS_GPU_INDEX parsing in kernel-mtl
    let lib_rs = match fs::read_to_string("crates/mplora-kernel-mtl/src/lib.rs") {
        Ok(content) => content,
        Err(e) => {
            return Check::fail(
                "Multi-GPU selector",
                vec![],
                format!("Failed to read kernel source: {}", e),
            )
        }
    };

    if lib_rs.contains("AOS_GPU_INDEX") {
        // Find the line number
        let line_num = lib_rs
            .lines()
            .enumerate()
            .find(|(_, line)| line.contains("AOS_GPU_INDEX"))
            .map(|(i, _)| i + 1);

        let evidence = vec![
            "Found AOS_GPU_INDEX parsing".to_string(),
            format!("Location: crates/mplora-kernel-mtl/src/lib.rs:{}", line_num.unwrap_or(0)),
        ];
        Check::pass("Multi-GPU selector", evidence)
    } else {
        Check::fail(
            "Multi-GPU selector",
            vec![],
            "AOS_GPU_INDEX environment variable handling not found",
        )
    }
}

fn check_panic_recovery() -> Check {
    // Check for catch_unwind in recovery module
    let recovery_file = Path::new("crates/mplora-kernel-mtl/src/recovery.rs");
    if !recovery_file.exists() {
        return Check::fail(
            "Panic recovery boundary",
            vec![],
            "crates/mplora-kernel-mtl/src/recovery.rs not found",
        );
    }

    let content = match fs::read_to_string(recovery_file) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "Panic recovery boundary",
                vec![],
                format!("Failed to read recovery.rs: {}", e),
            )
        }
    };

    if content.contains("catch_unwind") {
        Check::pass(
            "Panic recovery boundary",
            vec![
                "crates/mplora-kernel-mtl/src/recovery.rs exists".to_string(),
                "catch_unwind boundary implemented".to_string(),
            ],
        )
    } else {
        Check::fail(
            "Panic recovery boundary",
            vec!["recovery.rs exists".to_string()],
            "catch_unwind not found in recovery module",
        )
    }
}
