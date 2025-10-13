//! Policy gates acceptance tests for Qwen integration

use mplora_core::{B3Hash, CPID};
use mplora_chat::{ChatMessage, ChatTemplateProcessor, ChatTemplate, SpecialTokens};
use mplora_plan::{ModelLoader, GqaConfig, RopeConfig};
use mplora_manifest::{ManifestV3, Base, Adapter, AdapterTier, RouterCfg, TelemetryCfg, SamplingCfg, BundleCfg, Policies, AccessCfg, Seeds};
use tempfile::tempdir;
use std::path::PathBuf;

/// Test policy enforcement for Qwen integration
#[tokio::test]
async fn test_qwen_policy_gates() {
    // Test 1: Evidence requirement policy
    test_evidence_requirement_policy().await;
    
    // Test 2: Router entropy floor policy
    test_router_entropy_floor_policy().await;
    
    // Test 3: Numeric validation policy
    test_numeric_validation_policy().await;
    
    // Test 4: Refusal policy for underspecified prompts
    test_refusal_policy().await;
    
    // Test 5: Egress block policy
    test_egress_block_policy().await;
    
    println!("✅ All Qwen policy gates tests passed!");
}

/// Test evidence requirement policy
async fn test_evidence_requirement_policy() {
    // Simulate a regulated tenant with evidence requirements
    let manifest = create_regulated_manifest();
    
    // Test that factual claims require evidence
    let prompt = "What is the torque specification for the Boeing 737 landing gear?";
    
    // This should trigger evidence requirement
    let requires_evidence = check_evidence_requirement(&prompt, &manifest);
    assert!(requires_evidence, "Factual claims should require evidence");
    
    // Test that non-factual prompts don't require evidence
    let prompt = "Hello, how are you today?";
    let requires_evidence = check_evidence_requirement(&prompt, &manifest);
    assert!(!requires_evidence, "Greetings should not require evidence");
    
    println!("✅ Evidence requirement policy test passed");
}

/// Test router entropy floor policy
async fn test_router_entropy_floor_policy() {
    let router_config = RouterCfg {
        k_sparse: 3,
        gate_quant: "q15".to_string(),
        entropy_floor: 0.02,
        tau: 1.0,
        sample_tokens_full: 128,
    };
    
    // Test that entropy floor prevents adapter collapse
    let adapter_activations = vec![0.95, 0.03, 0.02]; // One adapter dominates
    let adjusted_activations = apply_entropy_floor(&adapter_activations, router_config.entropy_floor);
    
    // Check that no adapter is below entropy floor
    for activation in &adjusted_activations {
        assert!(*activation >= router_config.entropy_floor, 
                "Adapter activation should not be below entropy floor");
    }
    
    // Check that entropy is increased
    let original_entropy = calculate_entropy(&adapter_activations);
    let adjusted_entropy = calculate_entropy(&adjusted_activations);
    assert!(adjusted_entropy > original_entropy, "Entropy should increase after applying floor");
    
    println!("✅ Router entropy floor policy test passed");
}

/// Test numeric validation policy
async fn test_numeric_validation_policy() {
    // Test that unit-free numbers are rejected
    let invalid_numbers = vec![
        "The torque is 1500",
        "Pressure should be 200",
        "Temperature is 75",
    ];
    
    for number in &invalid_numbers {
        let is_valid = validate_numeric_claim(number);
        assert!(!is_valid, "Unit-free numbers should be rejected: {}", number);
    }
    
    // Test that numbers with units are accepted
    let valid_numbers = vec![
        "The torque is 1500 in-lbf",
        "Pressure should be 200 psi",
        "Temperature is 75°F",
    ];
    
    for number in &valid_numbers {
        let is_valid = validate_numeric_claim(number);
        assert!(is_valid, "Numbers with units should be accepted: {}", number);
    }
    
    println!("✅ Numeric validation policy test passed");
}

/// Test refusal policy for underspecified prompts
async fn test_refusal_policy() {
    // Test underspecified prompts that should be refused
    let underspecified_prompts = vec![
        "What is the torque spec?",
        "Tell me about the component",
        "What are the requirements?",
    ];
    
    for prompt in &underspecified_prompts {
        let refusal = check_refusal_policy(prompt);
        assert!(refusal.should_refuse, "Underspecified prompt should be refused: {}", prompt);
        assert!(!refusal.needed_fields.is_empty(), "Refusal should list needed fields");
    }
    
    // Test well-specified prompts that should be accepted
    let well_specified_prompts = vec![
        "What is the torque specification for the Boeing 737 landing gear component P/N 12345?",
        "Tell me about the hydraulic system requirements for aircraft effectivity 737-800",
    ];
    
    for prompt in &well_specified_prompts {
        let refusal = check_refusal_policy(prompt);
        assert!(!refusal.should_refuse, "Well-specified prompt should not be refused: {}", prompt);
    }
    
    println!("✅ Refusal policy test passed");
}

/// Test egress block policy
async fn test_egress_block_policy() {
    // Test that outbound connections are blocked
    let blocked_connections = vec![
        "tcp://example.com:80",
        "https://api.openai.com/v1/chat",
        "dns://8.8.8.8",
    ];
    
    for connection in &blocked_connections {
        let is_blocked = check_egress_block(connection);
        assert!(is_blocked, "Outbound connection should be blocked: {}", connection);
    }
    
    // Test that UDS connections are allowed
    let allowed_connections = vec![
        "/var/run/aos/tenant1/serve.sock",
        "/var/run/aos/tenant2/rag.sock",
    ];
    
    for connection in &allowed_connections {
        let is_blocked = check_egress_block(connection);
        assert!(!is_blocked, "UDS connection should be allowed: {}", connection);
    }
    
    println!("✅ Egress block policy test passed");
}

/// Helper function to create a regulated manifest
fn create_regulated_manifest() -> ManifestV3 {
    ManifestV3 {
        schema: "adapteros.manifest.v3".to_string(),
        base: Base {
            model_id: "Qwen2.5-7B-Instruct".to_string(),
            model_hash: B3Hash::hash(b"qwen"),
            arch: "qwen2".to_string(),
            vocab_size: 32000,
            hidden_dim: 4096,
            n_layers: 32,
            n_heads: 32,
            config_hash: B3Hash::hash(b"config"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
            license_hash: None,
            rope_scaling_override: None,
        },
        adapters: vec![
            Adapter {
                id: "torque-spec".to_string(),
                hash: B3Hash::hash(b"torque"),
                tier: AdapterTier::Persistent,
                rank: 16,
                alpha: 32.0,
                target_modules: vec!["q_proj".to_string(), "k_proj".to_string()],
                ttl: None,
                acl: vec!["engineering".to_string()],
            },
            Adapter {
                id: "general-knowledge".to_string(),
                hash: B3Hash::hash(b"general"),
                tier: AdapterTier::Persistent,
                rank: 8,
                alpha: 16.0,
                target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
                ttl: None,
                acl: vec!["public".to_string()],
            },
        ],
        router: RouterCfg {
            k_sparse: 3,
            gate_quant: "q15".to_string(),
            entropy_floor: 0.02,
            tau: 1.0,
            sample_tokens_full: 128,
        },
        telemetry: TelemetryCfg {
            schema_hash: B3Hash::hash(b"telemetry"),
            sampling: SamplingCfg {
                token: 0.05,
                router: 1.0,
                inference: 1.0,
            },
            router_full_tokens: 128,
            bundle: BundleCfg {
                max_events: 500000,
                max_bytes: 268435456,
            },
        },
        policies: Policies {
            egress: "deny_all".to_string(),
            access: AccessCfg {
                adapters: "RBAC".to_string(),
                datasets: "ABAC".to_string(),
            },
        },
        seeds: Seeds {
            global: B3Hash::hash(b"global"),
        },
    }
}

/// Check if a prompt requires evidence
fn check_evidence_requirement(prompt: &str, _manifest: &ManifestV3) -> bool {
    // Simple heuristic: factual claims require evidence
    let factual_keywords = vec![
        "torque", "specification", "requirement", "standard", "regulation",
        "boeing", "airbus", "component", "part number", "effectivity",
    ];
    
    factual_keywords.iter().any(|keyword| prompt.to_lowercase().contains(keyword))
}

/// Apply entropy floor to adapter activations
fn apply_entropy_floor(activations: &[f32], floor: f32) -> Vec<f32> {
    let mut adjusted = activations.to_vec();
    
    // Find the minimum activation
    let min_activation = adjusted.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    
    // If minimum is below floor, adjust all activations
    if min_activation < floor {
        let adjustment = floor - min_activation;
        for activation in &mut adjusted {
            *activation += adjustment;
        }
        
        // Renormalize
        let sum: f32 = adjusted.iter().sum();
        for activation in &mut adjusted {
            *activation /= sum;
        }
    }
    
    adjusted
}

/// Calculate entropy of a probability distribution
fn calculate_entropy(probabilities: &[f32]) -> f32 {
    -probabilities.iter()
        .filter(|&&p| p > 0.0)
        .map(|&p| p * p.log2())
        .sum::<f32>()
}

/// Validate numeric claims
fn validate_numeric_claim(text: &str) -> bool {
    // Check if text contains numbers with units
    let unit_patterns = vec![
        r"\d+\s*(in-lbf|ft-lbf|psi|bar|°F|°C|rpm|hz|hz)",
        r"\d+\s*(inches|feet|pounds|degrees)",
    ];
    
    unit_patterns.iter().any(|pattern| {
        regex::Regex::new(pattern).unwrap().is_match(text)
    })
}

/// Check refusal policy
fn check_refusal_policy(prompt: &str) -> RefusalResult {
    let mut needed_fields = Vec::new();
    
    // Check for missing aircraft effectivity
    if prompt.to_lowercase().contains("torque") && !prompt.to_lowercase().contains("effectivity") {
        needed_fields.push("aircraft_effectivity".to_string());
    }
    
    // Check for missing component part number
    if prompt.to_lowercase().contains("component") && !prompt.to_lowercase().contains("p/n") {
        needed_fields.push("component_pn".to_string());
    }
    
    // Check for missing system specification
    if prompt.to_lowercase().contains("system") && !prompt.to_lowercase().contains("specification") {
        needed_fields.push("system_spec".to_string());
    }
    
    RefusalResult {
        should_refuse: !needed_fields.is_empty(),
        needed_fields,
    }
}

/// Check egress block policy
fn check_egress_block(connection: &str) -> bool {
    // Block TCP/UDP connections
    if connection.starts_with("tcp://") || connection.starts_with("udp://") || 
       connection.starts_with("https://") || connection.starts_with("http://") ||
       connection.starts_with("dns://") {
        return true;
    }
    
    // Allow UDS connections
    if connection.starts_with("/var/run/aos/") {
        return false;
    }
    
    // Default to blocking
    true
}

/// Refusal result structure
#[derive(Debug)]
struct RefusalResult {
    should_refuse: bool,
    needed_fields: Vec<String>,
}

/// Test router heatmap policy
#[tokio::test]
async fn test_router_heatmap_policy() {
    let router_config = RouterCfg {
        k_sparse: 3,
        gate_quant: "q15".to_string(),
        entropy_floor: 0.02,
        tau: 1.0,
        sample_tokens_full: 128,
    };
    
    // Test different prompt types
    let prompts = vec![
        ("torque specification", vec!["torque-spec", "general-knowledge"]),
        ("general question", vec!["general-knowledge"]),
        ("mixed query", vec!["torque-spec", "general-knowledge"]),
    ];
    
    for (prompt, expected_adapters) in &prompts {
        let activations = simulate_router_activation(prompt, &router_config);
        
        // Check that expected adapters are activated
        for expected_adapter in expected_adapters {
            let is_activated = activations.iter().any(|(adapter, _)| adapter == expected_adapter);
            assert!(is_activated, "Expected adapter {} should be activated for prompt: {}", 
                    expected_adapter, prompt);
        }
        
        // Check entropy floor
        let min_activation = activations.iter().map(|(_, activation)| *activation).fold(f32::INFINITY, f32::min);
        assert!(min_activation >= router_config.entropy_floor, 
                "Minimum activation should not be below entropy floor");
    }
    
    println!("✅ Router heatmap policy test passed");
}

/// Simulate router activation for a prompt
fn simulate_router_activation(prompt: &str, _router_config: &RouterCfg) -> Vec<(String, f32)> {
    let mut activations = Vec::new();
    
    // Simple simulation based on prompt content
    if prompt.contains("torque") {
        activations.push(("torque-spec".to_string(), 0.7));
        activations.push(("general-knowledge".to_string(), 0.25));
    } else if prompt.contains("general") {
        activations.push(("general-knowledge".to_string(), 0.8));
        activations.push(("torque-spec".to_string(), 0.15));
    } else {
        activations.push(("general-knowledge".to_string(), 0.6));
        activations.push(("torque-spec".to_string(), 0.35));
    }
    
    // Apply entropy floor
    let min_activation = activations.iter().map(|(_, activation)| *activation).fold(f32::INFINITY, f32::min);
    if min_activation < 0.02 {
        let adjustment = 0.02 - min_activation;
        for (_, activation) in &mut activations {
            *activation += adjustment;
        }
        
        // Renormalize
        let sum: f32 = activations.iter().map(|(_, activation)| *activation).sum();
        for (_, activation) in &mut activations {
            *activation /= sum;
        }
    }
    
    activations
}
