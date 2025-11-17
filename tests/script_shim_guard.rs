use std::fs;
use std::path::Path;

/// Guardrail: selected `scripts/*.sh` files must be thin shims that
/// only print deprecation messages and delegate to Rust CLIs (`aos`/`aosctl`).
#[test]
fn deprecated_scripts_are_shims() {
    let scripts = [
        "scripts/aos.sh",
        "scripts/migrate.sh",
        "scripts/deploy_adapters.sh",
        "scripts/verify-determinism-loop.sh",
        "scripts/gc_bundles.sh",
    ];

    for script in scripts {
        let path = Path::new(script);
        assert!(
            path.exists(),
            "Expected deprecated shim script to exist: {script}"
        );

        let content =
            fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {script}: {e}"));

        let mut has_deprecation_message = false;
        let mut has_cli_exec = false;

        for raw_line in content.lines() {
            let line = raw_line.trim();

            // Ignore shebang, comments, and blank lines.
            if line.is_empty() || line.starts_with("#!") || line.starts_with('#') {
                continue;
            }

            // Allow strict mode.
            if line.starts_with("set ") {
                continue;
            }

            // Track deprecation messaging.
            if line.starts_with("echo ") && line.contains("deprecated") {
                has_deprecation_message = true;
            }

            // Track CLI delegation.
            if line.starts_with("exec aos ")
                || line == "exec aos \"$@\""
                || line.starts_with("exec aosctl ")
            {
                has_cli_exec = true;
                continue;
            }

            // Any other command (e.g., cp, rm, loops) would indicate real work.
            if !line.starts_with("echo ") {
                panic!(
                    "Script {script} contains non-shim logic:\n  offending line: `{}`",
                    raw_line
                );
            }
        }

        assert!(
            has_deprecation_message,
            "Script {script} should print a deprecation message containing the word 'deprecated'"
        );
        assert!(
            has_cli_exec,
            "Script {script} should delegate to a Rust CLI via an 'exec aos(...)' or 'exec aosctl(...)' line"
        );
    }
}
