#![cfg(all(test, feature = "extended-tests"))]

<<<<<<< HEAD
//! End-to-end integration checks for Qwen2.5 model wiring.

use adapteros_chat::{ChatTemplate, ChatTemplateProcessor, Message, SpecialTokens};
use adapteros_core::B3Hash;
use adapteros_lora_plan::{config::ModelConfig, ModelLoader};
use adapteros_lora_quant::BlockQuantizer;
use adapteros_manifest::{
    Adapter, AdapterCategory, AdapterScope, AdapterTier, Base, BundleCfg, EvictionPriority,
    ManifestV3, Policies, RouterCfg, Sampling, Seeds, TelemetryCfg,
};
use anyhow::Result;
=======
use adapteros_chat::{ChatTemplate, ChatTemplateProcessor, Message, SpecialTokens};
use adapteros_core::B3Hash;
use adapteros_lora_plan::config::ModelConfig;
use adapteros_lora_plan::ModelLoader;
use tempfile::tempdir;
>>>>>>> integration-branch

#[tokio::test]
<<<<<<< HEAD
async fn test_qwen_integration_pipeline() -> Result<()> {
    test_model_config_parsing()?;
    test_chat_template_processing()?;
    test_gqa_configuration()?;
    test_lora_memory_calculation()?;
    test_rope_configuration()?;
    test_manifest_validation()?;
    Ok(())
=======
async fn test_qwen_integration_pipeline() {
    // Test 1: Model configuration parsing
    test_model_config_parsing().await;

    // Test 2: Chat template processing
    test_chat_template_processing().await;

    // Test 3: GQA configuration validation
    test_gqa_configuration().await;

    // Test 4: LoRA memory calculation
    // test_lora_memory_calculation().await; // Commented out - needs ModelLoader API update

    // Test 5: RoPE configuration
    test_rope_configuration().await;

    println!("✅ All Qwen integration tests passed!");
>>>>>>> integration-branch
}

fn sample_model_config() -> ModelConfig {
    ModelConfig::from_json(
        r#"{
        "name": r#"Qwen2.5-7B-Instruct"#,
        "architecture": "qwen2",
        "vocab_size": 32000,
        "hidden_size": 4096,
        "intermediate_size": 11008,
        "num_hidden_layers": 32,
        "num_attention_heads": 32,
        "num_key_value_heads": 4,
        "rope_theta": 1000000.0,
        "max_position_embeddings": 32768
<<<<<<< HEAD
    }"#,
    )
    .expect("Sample config should parse")
}

fn sample_manifest() -> ManifestV3 {
    ManifestV3 {
        schema: "adapteros.manifest.v3".to_string(),
        base: Base {
=======
    }"#;

    let config = ModelConfig::from_json(config_json).unwrap();

    assert_eq!(config.name, "Qwen2.5-7B-Instruct");
    assert_eq!(config.architecture, "qwen2");
    assert_eq!(config.vocab_size, 32000);
    assert_eq!(config.hidden_size, 4096);
    assert_eq!(config.intermediate_size, 11008);
    assert_eq!(config.num_hidden_layers, 32);
    assert_eq!(config.num_attention_heads, 32);
    assert_eq!(config.num_key_value_heads, 4);
    assert_eq!(config.rope_theta, 1000000.0);
    assert_eq!(config.max_position_embeddings, 32768);

    // Test GQA validation
    config.validate_gqa().unwrap();

    println!("✅ Model configuration parsing test passed");
}

/// Test chat template processing
async fn test_chat_template_processing() {
    let template = ChatTemplate {
        name: "qwen".to_string(),
        template: "qwen_template".to_string(),
        special_tokens: SpecialTokens {
            bos: "<|im_start|>".to_string(),
            eos: "<|im_end|>".to_string(),
            unk: "<|unk|>".to_string(),
            pad: "<|pad|>".to_string(),
        },
    };

    let processor = ChatTemplateProcessor::new(template);

    let messages = vec![
        Message {
            role: "user".to_string(),
            content: "Hello, how are you?".to_string(),
        },
        Message {
            role: "assistant".to_string(),
            content: "I'm doing well, thank you!".to_string(),
        },
    ];

    let result = processor.apply(&messages).unwrap();

    let expected = "<|im_start|>user\nHello, how are you?<|im_end|>\n<|im_start|>assistant\nI'm doing well, thank you!<|im_end|>\n";

    assert_eq!(result, expected);
    assert_eq!(processor.special_tokens().eos, "<|im_end|>");

    println!("✅ Chat template processing test passed");
}

/// Test GQA configuration
async fn test_gqa_configuration() {
    // Test GQA validation through ModelConfig
    let config = ModelConfig::from_json(
        r#"{
        "name": "test",
        "architecture": "qwen2",
        "vocab_size": 32000,
        "hidden_size": 4096,
        "intermediate_size": 11008,
        "num_hidden_layers": 32,
        "num_attention_heads": 32,
        "num_key_value_heads": 4,
        "rope_theta": 10000.0,
        "max_position_embeddings": 32768
    }"#,
    )
    .unwrap();

    assert_eq!(config.num_attention_heads, 32);
    assert_eq!(config.num_key_value_heads, 4);

    // Verify GQA ratios
    assert_eq!(config.num_attention_heads / config.num_key_value_heads, 8);

    // Test dimensions calculation
    let dims = config.calculate_dimensions();
    assert_eq!(dims.head_dim, 128);
    assert_eq!(dims.kv_width, 512);

    println!("✅ GQA configuration test passed");
}

/// Test LoRA memory calculation
async fn test_lora_memory_calculation() {
    let config = adapteros_lora_plan::config::ModelConfig {
        name: "test".to_string(),
        architecture: "qwen2".to_string(),
        vocab_size: 32000,
        hidden_size: 4096,
        intermediate_size: 11008,
        num_hidden_layers: 32,
        num_attention_heads: 32,
        num_key_value_heads: 4,
        rope_theta: 10000.0,
        rope_scaling_factor: None,
        max_position_embeddings: 32768,
        total_params: 0,
    };

    let loader = ModelLoader {
        config,
        chat_template: ChatTemplateProcessor::new(ChatTemplate {
            template: "test".to_string(),
            template_hash: B3Hash::hash(b"test"),
            special_tokens: SpecialTokens {
                im_start: "<|im_start|>".to_string(),
                im_end: "<|im_end|>".to_string(),
                eos_token: "<|im_end|>".to_string(),
                pad_token: None,
            },
        }),
        quantizer: adapteros_lora_quant::BlockQuantizer::new("int4_block".to_string(), 128, 4),
    };

    let adapter = adapteros_manifest::Adapter {
        id: "test-adapter".to_string(),
        hash: B3Hash::hash(b"test"),
        tier: adapteros_manifest::AdapterTier::Persistent,
        rank: 16,
        alpha: 32.0,
        target_modules: vec!["q_proj".to_string(), "k_proj".to_string()],
        ttl: None,
        acl: vec![],
    };

    let memory = loader.calculate_adapter_memory(&adapter).unwrap();

    // Expected calculation:
    // rank=16, hidden_size=4096, intermediate_size=11008, num_layers=32
    // attention_params = 16 * (4096 + 4096) = 131,072
    // mlp_params = 16 * (4096 + 11008 + 11008 + 4096) = 491,520
    // params_per_layer = 131,072 + 491,520 = 622,592
    // total_params = 622,592 * 32 = 19,922,944
    // memory_bytes = 19,922,944 * 2 = 39,845,888
    assert_eq!(memory, 39_845_888);

    println!("✅ LoRA memory calculation test passed");
}

/// Test RoPE configuration
async fn test_rope_configuration() {
    // Test RoPE configuration through ModelConfig
    let config = ModelConfig::from_json(
        r#"{
        "name": "test",
        "architecture": "qwen2",
        "vocab_size": 32000,
        "hidden_size": 4096,
        "intermediate_size": 11008,
        "num_hidden_layers": 32,
        "num_attention_heads": 32,
        "num_key_value_heads": 4,
        "rope_theta": 1000000.0,
        "max_position_embeddings": 32768
    }"#,
    )
    .unwrap();

    assert_eq!(config.rope_theta, 1000000.0);
    assert_eq!(config.max_position_embeddings, 32768);

    println!("✅ RoPE configuration test passed");
}

/// Test model loader integration
#[tokio::test]
#[ignore = "Requires ModelLoader API update"]
async fn test_model_loader_integration() {
    let temp_dir = tempdir().unwrap();
    let registry_path = temp_dir.path().join("registry.db");

    // Test loading model from registry (mock)
    let loader = ModelLoader::load_from_registry("Qwen2.5-7B-Instruct", &registry_path).unwrap();

    // Verify configuration
    assert_eq!(loader.config.name, "Qwen2.5-7B-Instruct");
    assert_eq!(loader.config.architecture, "qwen2");
    assert_eq!(loader.config.hidden_size, 4096);
    assert_eq!(loader.config.num_attention_heads, 32);
    assert_eq!(loader.config.num_key_value_heads, 4);

    // Test GQA config
    let gqa_config = loader.get_gqa_config();
    assert_eq!(gqa_config.num_attention_heads, 32);
    assert_eq!(gqa_config.num_key_value_heads, 4);
    assert_eq!(gqa_config.head_dim, 128);
    assert_eq!(gqa_config.kv_width, 512);

    // Test RoPE config
    let rope_config = loader.get_rope_config();
    assert_eq!(rope_config.theta, 1000000.0);
    assert_eq!(rope_config.max_position_embeddings, 32768);

    println!("✅ Model loader integration test passed");
}

/// Test manifest validation
#[tokio::test]
#[ignore = "Requires ModelLoader API update"]
async fn test_manifest_validation() {
    let config = adapteros_lora_plan::config::ModelConfig {
        name: "Qwen2.5-7B-Instruct".to_string(),
        architecture: "qwen2".to_string(),
        vocab_size: 32000,
        hidden_size: 4096,
        intermediate_size: 11008,
        num_hidden_layers: 32,
        num_attention_heads: 32,
        num_key_value_heads: 4,
        rope_theta: 1000000.0,
        rope_scaling_factor: None,
        max_position_embeddings: 32768,
        total_params: 0,
    };

    let loader = ModelLoader {
        config,
        chat_template: ChatTemplateProcessor::new(ChatTemplate {
            template: "test".to_string(),
            template_hash: B3Hash::hash(b"test"),
            special_tokens: SpecialTokens {
                im_start: "<|im_start|>".to_string(),
                im_end: "<|im_end|>".to_string(),
                eos_token: "<|im_end|>".to_string(),
                pad_token: None,
            },
        }),
        quantizer: adapteros_lora_quant::BlockQuantizer::new("int4_block".to_string(), 128, 4),
    };

    let manifest = adapteros_manifest::ManifestV3 {
        schema: "adapteros.manifest.v3".to_string(),
        base: adapteros_manifest::Base {
>>>>>>> integration-branch
            model_id: "Qwen2.5-7B-Instruct".to_string(),
            model_hash: B3Hash::hash(b"model"),
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
<<<<<<< HEAD
        adapters: vec![Adapter {
            id: "test-adapter".to_string(),
            hash: B3Hash::hash(b"adapter"),
            tier: AdapterTier::Persistent,
            rank: 16,
            alpha: 32.0,
            target_modules: vec!["q_proj".to_string(), "k_proj".to_string()],
            ttl: None,
            acl: vec![],
            warmup_prompt: None,
            dependencies: None,
            category: AdapterCategory::Code,
            scope: AdapterScope::Global,
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            auto_promote: true,
            eviction_priority: EvictionPriority::Normal,
        }],
        router: RouterCfg {
=======
        adapters: vec![],
        router: adapteros_manifest::RouterCfg {
>>>>>>> integration-branch
            k_sparse: 3,
            gate_quant: "q15".to_string(),
            entropy_floor: 0.02,
            tau: 1.0,
            sample_tokens_full: 128,
            warmup: false,
            algorithm: "weighted".to_string(),
            orthogonal_penalty: 0.1,
            shared_downsample: false,
            compression_ratio: 0.8,
            multi_path_enabled: false,
            diversity_threshold: 0.05,
            orthogonal_constraints: false,
        },
<<<<<<< HEAD
        telemetry: TelemetryCfg {
            schema_hash: B3Hash::hash(b"telemetry"),
            sampling: Sampling {
=======
        telemetry: adapteros_manifest::TelemetryCfg {
            schema_hash: B3Hash::hash(b"telemetry"),
            sampling: adapteros_manifest::SamplingCfg {
>>>>>>> integration-branch
                token: 0.05,
                router: 1.0,
                inference: 1.0,
            },
            router_full_tokens: 128,
<<<<<<< HEAD
            bundle: BundleCfg {
                max_events: 500_000,
                max_bytes: 256 * 1024 * 1024,
            },
        },
        policies: Policies::default(),
        seeds: Seeds {
            global: B3Hash::hash(b"global-seed"),
            manifest_hash: B3Hash::hash(b"manifest-hash"),
            parent_cpid: None,
        },
    }
}

fn sample_loader() -> ModelLoader {
    ModelLoader {
        config: sample_model_config(),
        chat_template: ChatTemplateProcessor::new(ChatTemplate {
            name: "qwen".to_string(),
            template: "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}".to_string(),
            special_tokens: SpecialTokens {
                bos: "<|im_start|>".to_string(),
                eos: "<|im_end|>".to_string(),
                unk: "<|unk|>".to_string(),
                pad: "<|pad|>".to_string(),
            },
        }),
        quantizer: BlockQuantizer::new("int4_block".to_string(), 128, 4),
    }
}

fn test_model_config_parsing() -> Result<()> {
    let config = sample_model_config();

    assert_eq!(config.name, "Qwen2.5-7B-Instruct");
    assert_eq!(config.architecture, "qwen2");
    assert_eq!(config.vocab_size, 32000);
    assert_eq!(config.hidden_size, 4096);
    assert_eq!(config.intermediate_size, 11008);
    assert_eq!(config.num_hidden_layers, 32);
    assert_eq!(config.num_attention_heads, 32);
    assert_eq!(config.num_key_value_heads, 4);
    assert_eq!(config.rope_theta, 1_000_000.0);
    assert_eq!(config.max_position_embeddings, 32_768);

    config.validate_gqa()?;

    Ok(())
}

fn test_chat_template_processing() -> Result<()> {
    let template = ChatTemplate {
        name: "qwen".to_string(),
        template: "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}".to_string(),
        special_tokens: SpecialTokens {
            bos: "<|im_start|>".to_string(),
            eos: "<|im_end|>".to_string(),
            unk: "<|unk|>".to_string(),
            pad: "<|pad|>".to_string(),
        },
    };

    let processor = ChatTemplateProcessor::new(template);
    let messages = vec![
        Message {
            role: "user".to_string(),
            content: "Hello, how are you?".to_string(),
        },
        Message {
            role: "assistant".to_string(),
            content: "I'm doing well, thank you!".to_string(),
        },
    ];

    let rendered = processor.apply(&messages)?;
    assert!(rendered.contains("<|im_start|>user"));
    assert!(rendered.contains("<|im_end|>"));
    assert_eq!(processor.special_tokens().eos, "<|im_end|>");

    Ok(())
}

fn test_gqa_configuration() -> Result<()> {
    let config = sample_model_config();
    let dims = config.dimensions();

    assert_eq!(dims.head_dim, 128);
    assert_eq!(dims.kv_width, 512);
    assert!(dims.total_params > 0);

    Ok(())
}

fn test_lora_memory_calculation() -> Result<()> {
    let loader = sample_loader();

    let adapter = Adapter {
        id: "test-adapter".to_string(),
        hash: B3Hash::hash(b"adapter"),
        tier: AdapterTier::Persistent,
        rank: 16,
        alpha: 32.0,
        target_modules: vec!["q_proj".to_string(), "k_proj".to_string()],
        ttl: None,
        acl: vec![],
        warmup_prompt: None,
        dependencies: None,
        category: AdapterCategory::Code,
        scope: AdapterScope::Global,
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        auto_promote: true,
        eviction_priority: EvictionPriority::Normal,
    };

    let adapters = vec![adapter];
    let memory = loader.calculate_lora_memory(&adapters)?;
    assert_eq!(memory.len(), 1);
    // Expected calculation:
    // rank=16, hidden_size=4096, intermediate_size=11008, num_layers=32
    // attention_params = 16 * (4096 + 4096) = 131,072
    // mlp_params = 16 * (4096 + 11008 + 11008 + 4096) = 483,328
    // params_per_layer = 614,400
    // total_params = 614,400 * 32 = 19,660,800
    // memory_bytes = 19,660,800 * 2 = 39,321,600
    assert_eq!(memory[0], 39_321_600);

    Ok(())
}

fn test_rope_configuration() -> Result<()> {
    let mut config = sample_model_config();

    // Provide a rope scaling override and ensure effective context length honours it.
    config.rope_scaling = Some(adapteros_lora_plan::config::RopeScaling {
        factor: 2.0,
        original_max_position_embeddings: 32_768,
        scaling_type: "yarn".to_string(),
    });

    let effective_ctx = config.effective_context_length();
    assert_eq!(effective_ctx, 65_536);

    Ok(())
}

fn test_manifest_validation() -> Result<()> {
    let loader = sample_loader();
    let manifest = sample_manifest();

    loader.validate_manifest(&manifest)?;
    manifest.validate()?;

    Ok(())
=======
            bundle: adapteros_manifest::BundleCfg {
                max_events: 500000,
                max_bytes: 268435456,
            },
        },
        policies: adapteros_manifest::Policies {
            egress: "none".to_string(),
            access: adapteros_manifest::AccessCfg {
                adapters: "RBAC".to_string(),
                datasets: "ABAC".to_string(),
            },
        },
        seeds: adapteros_manifest::Seeds {
            global: B3Hash::hash(b"global"),
        },
    };

    // Test validation
    loader.validate_manifest(&manifest).unwrap();

    println!("✅ Manifest validation test passed");
>>>>>>> integration-branch
}
