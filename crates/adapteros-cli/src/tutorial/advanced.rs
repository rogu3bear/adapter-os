//! Advanced tutorial: serve, import, audit

use anyhow::Result;
use dialoguer::{Confirm, Input, Select};

pub async fn run(ci_mode: bool) -> Result<()> {
    print_banner();

    println!("This advanced tutorial covers:");
    println!("  1. Adapter registration");
    println!("  2. Kernel verification");
    println!("  3. Full system diagnostics with bundle export\n");

    if !ci_mode {
        let proceed = Confirm::new()
            .with_prompt("Ready to begin advanced tutorial?")
            .default(true)
            .interact()?;

        if !proceed {
            println!("Tutorial cancelled.");
            return Ok(());
        }
    }

    // Step 1: Register Adapter
    step_1_register_adapter(ci_mode).await?;

    // Step 2: Verify Kernel
    step_2_verify_kernel(ci_mode).await?;

    // Step 3: Full Diagnostics
    step_3_full_diagnostics(ci_mode).await?;

    print_summary();

    Ok(())
}

fn print_banner() {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("           AdapterOS Interactive Tutorial (Advanced)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
}

async fn step_1_register_adapter(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 1: Register an Adapter");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Adapters are LoRA weights that specialize the base model.");
    println!("Registration requires:");
    println!("  - Adapter ID (unique identifier)");
    println!("  - Artifact hash (BLAKE3, from CAS)");
    println!("  - Tier (persistent or ephemeral)");
    println!("  - Rank (LoRA rank, typically 4-32)");
    println!();

    let (adapter_id, hash, tier, rank) = if ci_mode {
        (
            "tutorial_adapter".to_string(),
            "b3:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            "ephemeral".to_string(),
            8,
        )
    } else {
        let adapter_id: String = Input::new()
            .with_prompt("Adapter ID")
            .default("tutorial_adapter".to_string())
            .interact_text()?;

        let hash: String = Input::new()
            .with_prompt("Artifact hash (b3:...)")
            .default("b3:0000000000000000000000000000000000000000000000000000000000000000".to_string())
            .interact_text()?;

        let tier_options = vec!["ephemeral", "persistent"];
        let tier_idx = Select::new()
            .with_prompt("Tier")
            .items(&tier_options)
            .default(0)
            .interact()?;
        let tier = tier_options[tier_idx].to_string();

        let rank: u32 = Input::new()
            .with_prompt("LoRA rank")
            .default(8)
            .interact()?;

        (adapter_id, hash, tier, rank)
    };

    println!("\n📝 Command that would run:");
    println!("   aosctl register-adapter {} {} --tier {} --rank {}",
        adapter_id, hash, tier, rank);
    println!();

    println!("💡 Adapter Lifecycle:");
    println!("   - Ephemeral: TTL-based, auto-evicted when cold");
    println!("   - Persistent: Survives eviction, requires explicit removal");
    println!("   - Router selects top-K adapters per token");
    println!("   - Activation below min_activation_pct triggers eviction");
    println!();

    println!("📊 Router Ruleset (from policy):");
    println!("   - k_sparse: 3 (max adapters per token)");
    println!("   - gate_quant: Q15 (quantized gates)");
    println!("   - entropy_floor: 0.02 (prevent collapse)");

    if !ci_mode {
        Confirm::new()
            .with_prompt("Continue to next step?")
            .default(true)
            .interact()?;
    }

    Ok(())
}

async fn step_2_verify_kernel(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 2: Verify Kernel Build");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("AdapterOS uses fused Metal kernels for performance.");
    println!("Determinism requires:");
    println!("  - Precompiled .metallib (no runtime compilation)");
    println!("  - Kernel hash embedded in Plan manifest");
    println!("  - Toolchain version tracked");
    println!("  - No fast-math flags");
    println!();

    println!("📝 Kernel build process:");
    println!("   cd metal");
    println!("   ./build.sh              # Compile kernels");
    println!("   ./ci_build.sh           # CI determinism check");
    println!();

    println!("📦 Kernel artifacts:");
    println!("   - aos_kernels.metallib       (compiled library)");
    println!("   - aos_kernels.metallib.sig   (Ed25519 signature)");
    println!("   - toolchain.toml             (build metadata)");
    println!();

    println!("🔍 Verification checks:");
    println!("   1. Kernel file exists");
    println!("   2. Signature validates");
    println!("   3. Hash matches Plan manifest");
    println!("   4. Toolchain version recorded");
    println!();

    let kernel_path = std::path::Path::new("./metal/aos_kernels.metallib");
    if kernel_path.exists() {
        println!("✅ Kernel found: {}", kernel_path.display());
        
        let sig_path = std::path::Path::new("./metal/aos_kernels.metallib.sig");
        if sig_path.exists() {
            println!("✅ Signature found: {}", sig_path.display());
        } else {
            println!("⚠ Signature not found (run: cd metal && ./build.sh)");
        }
    } else {
        println!("⚠ Kernel not found");
        println!("   Build: cd metal && ./build.sh");
    }

    println!();
    println!("💡 Determinism Policy:");
    println!("   - Serving refuses if kernel hash mismatch (E3002)");
    println!("   - Replay must produce identical outputs (E2001)");
    println!("   - RNG seeded with HKDF derivation");

    if !ci_mode {
        Confirm::new()
            .with_prompt("Continue to next step?")
            .default(true)
            .interact()?;
    }

    Ok(())
}

async fn step_3_full_diagnostics(ci_mode: bool) -> Result<()> {
    println!("\n📘 Step 3: Full System Diagnostics");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Full diagnostics check everything:");
    println!("  • System: Metal, memory, disk, permissions");
    println!("  • Tenant: registry, adapters, policies");
    println!("  • Services: aos-secd, worker, heartbeat");
    println!();

    let bundle_path = if ci_mode {
        Some("./diag-tutorial.zip".to_string())
    } else {
        let create_bundle = Confirm::new()
            .with_prompt("Create diagnostic bundle?")
            .default(true)
            .interact()?;

        if create_bundle {
            Some(Input::new()
                .with_prompt("Bundle path")
                .default("./diag-tutorial.zip".to_string())
                .interact_text()?)
        } else {
            None
        }
    };

    println!("\n📝 Command:");
    if let Some(ref path) = bundle_path {
        println!("   aosctl diag --full --bundle {}", path);
    } else {
        println!("   aosctl diag --full");
    }
    println!();

    if ci_mode {
        println!("✓ [CI Mode] Running full diagnostics...\n");
    } else {
        let execute = Confirm::new()
            .with_prompt("Run full diagnostics now?")
            .default(true)
            .interact()?;

        if !execute {
            println!("✓ Skipped");
            return Ok(());
        }

        println!("\nExecuting...\n");
    }

    // Run full diagnostics
    let bundle = bundle_path.as_ref().map(std::path::PathBuf::from);
    match crate::commands::diag::run(
        crate::commands::diag::DiagProfile::Full,
        None,
        false,
        bundle.clone(),
    ).await {
        Ok(_) => {
            if let Some(path) = bundle {
                println!("\n📦 Diagnostic bundle created: {}", path.display());
                println!("   Contents:");
                println!("     - diagnostics.json    (check results)");
                println!("     - system_info.json    (OS, memory, CPU)");
                println!("     - configs/            (policy packs)");
                println!("     - logs/               (recent events)");
            }
        },
        Err(e) => {
            println!("\n⚠ Diagnostics completed with issues: {}", e);
            println!("   This is normal for a dev environment.");
        }
    }

    println!("\n💡 Exit Codes:");
    println!("   0  - All checks passed");
    println!("   10 - Warnings present");
    println!("   20 - Failures detected");
    println!();
    println!("📚 Error Help:");
    println!("   aosctl explain E3004    (Metal device not found)");
    println!("   aosctl explain E9001    (insufficient memory)");
    println!("   aosctl error-codes      (list all codes)");

    Ok(())
}

fn print_summary() {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("            Advanced Tutorial Complete! 🚀");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("You've learned:");
    println!("  ✓ How to register adapters with tiers and ranks");
    println!("  ✓ How kernel verification ensures determinism");
    println!("  ✓ How to run comprehensive diagnostics");
    println!("  ✓ How to create support bundles");
    println!();
    println!("Production checklist:");
    println!("  □ Build kernels with signatures");
    println!("  □ Configure policy packs (configs/cp.toml)");
    println!("  □ Set up aos-secd service");
    println!("  □ Initialize tenants with proper UID/GID");
    println!("  □ Import and verify all artifacts");
    println!("  □ Run audit before CP promotion");
    println!("  □ Test determinism with replay");
    println!("  □ Configure telemetry retention");
    println!();
    println!("Key commands:");
    println!("  • aosctl serve --tenant <id> --plan <cpid>");
    println!("  • aosctl audit <cpid> --suite tests/corpora/");
    println!("  • aosctl replay <bundle> --verbose");
    println!("  • aosctl rollback --tenant <id> <cpid>");
    println!();
    println!("Documentation:");
    println!("  • aosctl manual                   (offline docs)");
    println!("  • docs/architecture.md            (system design)");
    println!("  • docs/control-plane.md           (CP lifecycle)");
    println!("  • docs/metal/phase4-metal-kernels.md (kernel impl)");
    println!();
}

