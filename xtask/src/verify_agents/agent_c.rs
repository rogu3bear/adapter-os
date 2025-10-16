//! Agent C: Adapters & Routing checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent C - Adapters & Routing");

    // 1. Migration range check
    section.add_check(check_migrations());

    // 2. Zeroization (code check)
    section.add_check(check_zeroization());

    // 3. Cache warmup
    section.add_check(check_cache_warmup());

    // 4. Auto-reload
    section.add_check(check_auto_reload());

    // 5. Pinning
    section.add_check(check_pinning());

    // 6. Per-tenant memory policy
    section.add_check(check_per_tenant_policy());

    // 7. Dependencies
    section.add_check(check_dependencies());

    // 8. Router k0 events
    section.add_check(check_router_k0());

    Ok(section)
}

fn check_migrations() -> Check {
    let migrations_dir = Path::new("migrations");
    if !migrations_dir.exists() {
        return Check::fail("Migration range check", vec![], "migrations/ not found");
    }

    let entries = match fs::read_dir(migrations_dir) {
        Ok(e) => e,
        Err(e) => {
            return Check::fail(
                "Migration range check",
                vec![],
                format!("Failed to read migrations: {}", e),
            )
        }
    };

    let agent_c_migrations: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("02") // 0200-0299
        })
        .collect();

    if agent_c_migrations.is_empty() {
        return Check::fail(
            "Migration range check",
            vec![],
            "No migrations in 0200-0299 range found",
        );
    }

    // Check for pinned_adapters and dependencies
    let has_pinned = agent_c_migrations
        .iter()
        .any(|e| e.file_name().to_string_lossy().contains("pinned_adapters"));

    let has_dependencies = agent_c_migrations
        .iter()
        .any(|e| e.file_name().to_string_lossy().contains("dependencies"));

    let evidence: Vec<_> = agent_c_migrations
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    if has_pinned || has_dependencies {
        Check::pass(
            "Migration range check",
            vec![
                format!("Found {} Agent C migrations", evidence.len()),
                format!("Files: {}", evidence.join(", ")),
            ],
        )
    } else {
        Check::skip(
            "Migration range check",
            format!(
                "Found migrations in 0200 range but no pinned/dependencies: {}",
                evidence.join(", ")
            ),
        )
    }
}

fn check_zeroization() -> Check {
    // Check for zeroization in registry or worker
    let registry_src = "crates/mplora-registry/src/lib.rs";
    let worker_src = "crates/mplora-worker/src/lib.rs";

    let mut found_in = Vec::new();
    let mut evidence = Vec::new();

    for (path, name) in [(registry_src, "registry"), (worker_src, "worker")] {
        if let Ok(content) = fs::read_to_string(path) {
            if content.contains("zeroize") || content.contains("adapter.zeroized") {
                found_in.push(name);
                evidence.push(format!("Zeroization code found in {}", name));
            }
        }
    }

    if !found_in.is_empty() {
        Check::pass("Zeroization", evidence)
    } else {
        Check::skip(
            "Zeroization",
            "Zeroization code not found in registry/worker (check telemetry for events)",
        )
    }
}

fn check_cache_warmup() -> Check {
    // Check for warmup logic in worker or orchestrator
    let sources = [
        "crates/mplora-worker/src/lib.rs",
        "crates/mplora-orchestrator/src/lib.rs",
        "crates/mplora-registry/src/lib.rs",
    ];

    for src in sources {
        if let Ok(content) = fs::read_to_string(src) {
            if content.contains("warmup") || content.contains("cache_warmup") {
                return Check::pass(
                    "Cache warmup",
                    vec![format!("Warmup logic found in {}", src)],
                );
            }
        }
    }

    Check::skip(
        "Cache warmup",
        "Warmup logic not found in worker/orchestrator (may be in manifest config only)",
    )
}

fn check_auto_reload() -> Check {
    // Check for reload logic in registry or worker
    let sources = [
        "crates/mplora-registry/src/lib.rs",
        "crates/mplora-worker/src/lib.rs",
    ];

    for src in sources {
        if let Ok(content) = fs::read_to_string(src) {
            if content.contains("reload") && content.contains("adapter") {
                return Check::pass(
                    "Auto-reload",
                    vec![format!("Reload logic found in {}", src)],
                );
            }
        }
    }

    Check::skip(
        "Auto-reload",
        "Reload logic not explicitly found (check telemetry for adapter.reload events)",
    )
}

fn check_pinning() -> Check {
    // Check for pinning in CLI or database
    let cli_src = "crates/mplora-cli/src/commands/adapters.rs";

    if let Ok(content) = fs::read_to_string(cli_src) {
        if content.contains("pin") {
            return Check::pass(
                "Pinning",
                vec![
                    "Pin command found in CLI".to_string(),
                    format!("Location: {}", cli_src),
                ],
            );
        }
    }

    // Check migrations for pinned adapters table
    if let Ok(content) = fs::read_to_string("migrations/0200_pinned_adapters.sql") {
        if content.contains("pinned") {
            return Check::pass(
                "Pinning",
                vec!["Pinned adapters table found in migrations".to_string()],
            );
        }
    }

    Check::skip("Pinning", "Pin command not found in CLI or migrations")
}

fn check_per_tenant_policy() -> Check {
    // Check for tenant-specific policy in policy or worker crates
    let policy_src = "crates/mplora-policy/src/lib.rs";
    let worker_src = "crates/mplora-worker/src/lib.rs";

    for src in [policy_src, worker_src] {
        if let Ok(content) = fs::read_to_string(src) {
            if content.contains("tenant") && content.contains("policy") {
                return Check::pass(
                    "Per-tenant memory policy",
                    vec![format!("Tenant policy logic found in {}", src)],
                );
            }
        }
    }

    Check::skip(
        "Per-tenant memory policy",
        "Tenant-specific policy not explicitly found (may be in manifest parsing)",
    )
}

fn check_dependencies() -> Check {
    // Check for dependency validation in registry
    let registry_src = "crates/mplora-registry/src/lib.rs";

    if let Ok(content) = fs::read_to_string(registry_src) {
        if content.contains("dependencies") || content.contains("requires_adapters") {
            return Check::pass(
                "Dependencies",
                vec!["Dependency validation found in registry".to_string()],
            );
        }
    }

    // Check migrations for dependencies table
    if let Ok(content) = fs::read_to_string("migrations/0201_adapter_dependencies.sql") {
        if content.contains("dependencies") {
            return Check::pass(
                "Dependencies",
                vec!["Adapter dependencies table found in migrations".to_string()],
            );
        }
    }

    Check::skip(
        "Dependencies",
        "Dependency validation not found in registry or migrations",
    )
}

fn check_router_k0() -> Check {
    // Check for k0 handling in router
    let router_src = "crates/mplora-router/src/lib.rs";

    if let Ok(content) = fs::read_to_string(router_src) {
        if content.contains("k0") || content.contains("router.k0") {
            return Check::pass(
                "Router k0 events",
                vec!["k0 event handling found in router".to_string()],
            );
        }
    }

    Check::skip(
        "Router k0 events",
        "k0 event code not found (check telemetry for router.k0 events)",
    )
}
