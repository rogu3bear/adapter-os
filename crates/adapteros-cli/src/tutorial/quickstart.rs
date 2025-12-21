//! Quickstart tutorial: init, verify, diag

use anyhow::Result;
use dialoguer::{Confirm, Input};
use crate::output::{OutputWriter, OutputMode};

pub async fn run(ci_mode: bool) -> Result<()> {
    print_banner();

    println!("This tutorial will walk you through the basics of aosctl:");
    println!("  1. Initialize a tenant");
    println!("  2. Verify artifacts");
    println!("  3. Run system diagnostics\n");

    if !ci_mode {
        let proceed = Confirm::new()
            .with_prompt("Ready to begin?")
            .default(true)
            .interact()?;

        if !proceed {
            println!("Tutorial cancelled.");
            return Ok(());
        }
    }

    // Step 1: Init Tenant
    step_1_init_tenant(ci_mode).await?;

    // Step 2: Verify
    step_2_verify(ci_mode).await?;

    // Step 3: Diagnostics
    step_3_diagnostics(ci_mode).await?;

    print_summary();

    Ok(())
}

fn print_banner() {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("           AdapterOS Interactive Tutorial (Quickstart)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
}

async fn step_1_init_tenant(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 1: Initialize a Tenant");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Tenants provide isolation between different users or environments.");
    println!("Each tenant has a unique ID, UID, and GID for process isolation.");
    println!();

    let (tenant_id, uid, gid) = if ci_mode {
        ("tutorial_test".to_string(), 1000, 1000)
    } else {
        let tenant_id: String = Input::new()
            .with_prompt("Tenant ID")
            .default("tutorial_test".to_string())
            .interact_text()?;

        let uid: u32 = Input::new()
            .with_prompt("Unix UID")
            .default(1000)
            .interact()?;

        let gid: u32 = Input::new()
            .with_prompt("Unix GID")
            .default(1000)
            .interact()?;

        (tenant_id, uid, gid)
    };

    println!("\n📝 Command that would run:");
    println!("   aosctl init-tenant --id {} --uid {} --gid {}", tenant_id, uid, gid);
    println!();

    if ci_mode {
        println!("✓ [CI Mode] Tenant initialization skipped (dry run)");
    } else {
        let execute = Confirm::new()
            .with_prompt("Execute this command? (No = dry run)")
            .default(false)
            .interact()?;

        if execute {
            println!("\nExecuting...");
            let output = OutputWriter::new(OutputMode::Human, false);
            match crate::commands::init_tenant::run(&tenant_id, uid, gid, &output).await {
                Ok(_) => println!("✅ Tenant initialized successfully!"),
                Err(e) => {
                    println!("⚠ Command failed: {}", e);
                    println!("   This is okay for a tutorial - the tenant might already exist.");
                }
            }
        } else {
            println!("✓ Skipped (dry run)");
        }
    }

    println!("\n💡 Key Concepts:");
    println!("   - Tenants are isolated by Unix UID/GID");
    println!("   - Each tenant has its own UDS socket path");
    println!("   - Tenant data is stored in /var/run/aos/<tenant_id>");

    if !ci_mode {
        Confirm::new()
            .with_prompt("Continue to next step?")
            .default(true)
            .interact()?;
    }

    Ok(())
}

async fn step_2_verify(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 2: Verify Artifacts");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("AdapterOS requires all artifacts to be signed and hashed.");
    println!("The verify command checks signatures, SBOM, and artifact hashes.");
    println!();

    println!("Example commands:");
    println!("   aosctl verify <bundle.zip>");
    println!("   aosctl verify-telemetry --bundle-dir ./var/telemetry");
    println!();

    println!("📦 Artifact verification ensures:");
    println!("   - Ed25519 signature is valid");
    println!("   - SBOM (Software Bill of Materials) is complete");
    println!("   - All artifact hashes match (BLAKE3)");
    println!("   - No tampering or corruption");
    println!();

    println!("💡 Policy Enforcement:");
    println!("   The egress ruleset (E2003) requires signature+SBOM");
    println!("   for all imports. No network access during serving.");

    if !ci_mode {
        Confirm::new()
            .with_prompt("Continue to next step?")
            .default(true)
            .interact()?;
    }

    Ok(())
}

async fn step_3_diagnostics(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 3: System Diagnostics");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("The diag command performs comprehensive system checks.");
    println!();

    println!("📝 Command:");
    println!("   aosctl diag --system");
    println!();

    if ci_mode {
        println!("✓ [CI Mode] Running diagnostics...\n");
    } else {
        let execute = Confirm::new()
            .with_prompt("Run diagnostics now?")
            .default(true)
            .interact()?;

        if !execute {
            println!("✓ Skipped");
            return Ok(());
        }

        println!("\nExecuting...\n");
    }

    // Actually run diagnostics
    match crate::commands::diag::run(
        crate::commands::diag::DiagProfile::System,
        None,
        false,
        None,
    ).await {
        Ok(_) => {},
        Err(e) => {
            println!("\n⚠ Diagnostics completed with issues: {}", e);
            println!("   This is normal for a dev environment.");
        }
    }

    println!("\n💡 Diagnostic Profiles:");
    println!("   --system   : Metal, memory, disk, permissions, kernels");
    println!("   --tenant   : Registry, adapters, policies, telemetry");
    println!("   --full     : All of the above + service checks");
    println!();
    println!("📦 Bundle mode:");
    println!("   aosctl diag --full --bundle diag.zip");
    println!("   Creates a support bundle with logs, configs, and metrics");

    Ok(())
}

fn print_summary() {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("                Tutorial Complete! 🎉");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("You've learned:");
    println!("  ✓ How to initialize tenants");
    println!("  ✓ How to verify artifacts and signatures");
    println!("  ✓ How to run system diagnostics");
    println!();
    println!("Next steps:");
    println!("  • aosctl tutorial --advanced    (serve, import, audit)");
    println!("  • aosctl manual                 (offline documentation)");
    println!("  • aosctl explain <CODE>         (error code lookup)");
    println!("  • aosctl --help                 (command reference)");
    println!();
    println!("For production deployment:");
    println!("  • Review docs/architecture.md");
    println!("  • Set up aos-secd service");
    println!("  • Configure policy packs in configs/cp.toml");
    println!("  • Build kernels: cd metal && ./build.sh");
    println!();
}

