use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn no_tmp_usage_in_runtime_sources() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();

    let targets = [
        repo_root.join("crates/adapteros-lora-worker/src"),
        repo_root.join("crates/adapteros-server-api/src"),
        repo_root.join("crates/adapteros-orchestrator/src"),
    ];

    let mut violations = Vec::new();
    for dir in targets {
        scan_dir(&dir, &mut violations);
    }

    if !violations.is_empty() {
        panic!(
            "Found disallowed /tmp usage:\n{}",
            violations
                .into_iter()
                .map(|(path, line)| format!("{}:{}", path.display(), line))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn scan_dir(dir: &Path, violations: &mut Vec<(PathBuf, String)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.ends_with("tests") {
                continue;
            }
            scan_dir(&path, violations);
            continue;
        }

        if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
            if let Ok(contents) = fs::read_to_string(&path) {
                for line in contents.lines() {
                    if is_comment_line(line) {
                        continue;
                    }
                    if has_disallowed_tmp(line) {
                        violations.push((path.clone(), line.to_string()));
                        break;
                    }
                }
            }
        }
    }
}

fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//")
        || trimmed.starts_with("///")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
}

fn has_disallowed_tmp(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("/tmp") {
        let idx = start + pos;
        let is_var_tmp = idx >= 3 && &line[idx - 3..idx + 4] == "var/tmp";
        if !is_var_tmp {
            return true;
        }
        start = idx + 4;
    }
    false
}
