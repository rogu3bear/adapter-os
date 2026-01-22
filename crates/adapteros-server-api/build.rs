//! Build script for adapteros-server-api
//!
//! Generates a unified build ID combining git commit hash and build timestamp.
//! Format: {7-char-git-hash}-{YYYYMMDDHHmmss} e.g., "a6922d2-20260122153045"

use std::process::Command;

fn main() {
    // Rerun if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let git_hash = get_git_hash();
    let timestamp = get_build_timestamp();

    let build_id = format!("{}-{}", git_hash, timestamp);
    println!("cargo:rustc-env=AOS_BUILD_ID={}", build_id);
}

fn get_git_hash() -> String {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    "unknown".to_string()
}

fn get_build_timestamp() -> String {
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = epoch.parse::<i64>() {
            if let Ok(output) = Command::new("date")
                .args(["-u", "-r", &secs.to_string(), "+%Y%m%d%H%M%S"])
                .output()
            {
                if output.status.success() {
                    return String::from_utf8_lossy(&output.stdout).trim().to_string();
                }
            }
        }
    }

    if let Ok(output) = Command::new("date").args(["-u", "+%Y%m%d%H%M%S"]).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    "00000000000000".to_string()
}
