use std::fs;
use std::path::Path;

/// Guardrail: every `scripts/*.sh` file must be explicitly referenced in
/// either `docs/internal/cli-inventory.md` or `DEPRECATIONS.md`.
///
/// This prevents new shell scripts from bypassing the documented CLI plan.
#[test]
fn scripts_are_listed_in_inventory_or_deprecations() {
    let scripts_dir = Path::new("scripts");
    if !scripts_dir.exists() {
        // If the scripts directory does not exist, there is nothing to enforce.
        return;
    }

    let inventory = fs::read_to_string("docs/internal/cli-inventory.md")
        .expect("docs/internal/cli-inventory.md must exist");
    let deprecations = fs::read_to_string("DEPRECATIONS.md").expect("DEPRECATIONS.md must exist");

    let mut missing: Vec<String> = Vec::new();

    for entry in fs::read_dir(scripts_dir).expect("Failed to read scripts directory") {
        let entry = entry.expect("Failed to read scripts entry");
        let path = entry.path();

        // Only check `.sh` scripts as per the guardrail definition.
        if path.extension().and_then(|e| e.to_str()) != Some("sh") {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let listed_in_inventory = inventory.contains(&file_name);
        let listed_in_deprecations = deprecations.contains(&file_name);

        if !listed_in_inventory && !listed_in_deprecations {
            missing.push(file_name);
        }
    }

    if !missing.is_empty() {
        panic!(
            "The following scripts/*.sh files are not listed in \
             docs/internal/cli-inventory.md or DEPRECATIONS.md: {missing:?}. \
             Please add each script name to at least one of these documents."
        );
    }
}
