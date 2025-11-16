//! Baseline checks: fmt, clippy, check

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::process::Command;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Baseline - Build & Static Checks");

    // cargo fmt --all -- --check
    let fmt_check = run_fmt_check();
    section.add_check(fmt_check);

    // cargo clippy --all-features -- -D warnings
    let clippy_check = run_clippy_check();
    section.add_check(clippy_check);

    // cargo check --all-features
    let check_check = run_cargo_check();
    section.add_check(check_check);

    Ok(section)
}

fn run_fmt_check() -> Check {
    let output = Command::new("cargo")
        .args(["fmt", "--all", "--", "--check"])
        .output();

    match output {
        Ok(out) if out.status.success() => Check::pass(
            "cargo fmt",
            vec!["All files properly formatted".to_string()],
        ),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Check::fail(
                "cargo fmt",
                vec![stderr.to_string()],
                "Code formatting issues detected",
            )
        }
        Err(e) => Check::fail("cargo fmt", vec![], format!("Failed to run: {}", e)),
    }
}

fn run_clippy_check() -> Check {
    let output = Command::new("cargo")
        .args(["clippy", "--all-features", "--", "-D", "warnings"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            Check::pass("cargo clippy", vec!["No clippy warnings".to_string()])
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            Check::fail(
                "cargo clippy",
                vec![format!("{}\n{}", stdout, stderr)],
                "Clippy warnings detected",
            )
        }
        Err(e) => Check::fail("cargo clippy", vec![], format!("Failed to run: {}", e)),
    }
}

fn run_cargo_check() -> Check {
    let output = Command::new("cargo")
        .args(["check", "--all-features"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            Check::pass("cargo check", vec!["All crates compile".to_string()])
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Check::fail(
                "cargo check",
                vec![stderr.to_string()],
                "Compilation errors detected",
            )
        }
        Err(e) => Check::fail("cargo check", vec![], format!("Failed to run: {}", e)),
    }
}
