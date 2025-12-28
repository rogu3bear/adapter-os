//! Metal Engine Harness
//!
//! A standalone binary to verify the stability of the Metal backend.
//! Loads a dummy model (random weights) and dummy adapter, then runs
//! a basic inference loop to check for crashes or memory errors.

#[cfg(target_os = "macos")]
mod harness {
    use adapteros_core::{AosError, Result};
    use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
    use adapteros_lora_kernel_mtl::{GqaConfig, MetalKernels, RingBuffer};
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use safetensors::serialize;
    use safetensors::tensor::{Dtype, TensorView};

    const HIDDEN_SIZE: usize = 64;
    const INTERMEDIATE_SIZE: usize = 128;
    const VOCAB_SIZE: usize = 32;
    const NUM_ATTENTION_HEADS: usize = 8;
    const NUM_KV_HEADS: usize = 8;
    const LORA_RANK: usize = 4;
    const ADAPTER_ID: u16 = 0;

    struct TensorSpec {
        name: String,
        shape: Vec<usize>,
        data: Vec<u8>,
    }

    fn random_f32_bytes(rng: &mut StdRng, len: usize) -> Vec<u8> {
        (0..len)
            .map(|_| rng.gen::<f32>())
            .flat_map(f32::to_le_bytes)
            .collect()
    }

    fn make_tensor(name: impl Into<String>, shape: Vec<usize>, rng: &mut StdRng) -> TensorSpec {
        let elements: usize = shape.iter().product();
        TensorSpec {
            name: name.into(),
            shape,
            data: random_f32_bytes(rng, elements),
        }
    }

    fn serialize_tensors(tensors: Vec<TensorSpec>) -> Result<Vec<u8>> {
        let mut views = Vec::with_capacity(tensors.len());
        for tensor in &tensors {
            let view = TensorView::new(Dtype::F32, tensor.shape.clone(), tensor.data.as_slice())
                .map_err(|e| {
                    AosError::Kernel(format!("Failed to build tensor {}: {}", tensor.name, e))
                })?;
            views.push((tensor.name.as_str(), view));
        }

        serialize(views, &None)
            .map_err(|e| AosError::Kernel(format!("Failed to serialize tensors: {}", e)))
    }

    /// Create a minimal SafeTensors plan with small, deterministic weights.
    #[allow(clippy::vec_init_then_push)]
    fn create_dummy_plan() -> Result<Vec<u8>> {
        let mut rng = StdRng::seed_from_u64(42);

        let mut tensors = Vec::new();

        // Embeddings + LM head
        tensors.push(make_tensor(
            "model.embed_tokens.weight",
            vec![VOCAB_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));
        tensors.push(make_tensor(
            "lm_head.weight",
            vec![VOCAB_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));

        // Layer 0 MLP weights
        tensors.push(make_tensor(
            "model.layers.0.mlp.gate_proj.weight",
            vec![HIDDEN_SIZE, INTERMEDIATE_SIZE],
            &mut rng,
        ));
        tensors.push(make_tensor(
            "model.layers.0.mlp.up_proj.weight",
            vec![HIDDEN_SIZE, INTERMEDIATE_SIZE],
            &mut rng,
        ));
        tensors.push(make_tensor(
            "model.layers.0.mlp.down_proj.weight",
            vec![INTERMEDIATE_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));

        // Layer 0 QKV weights
        tensors.push(make_tensor(
            "model.layers.0.self_attn.q_proj.weight",
            vec![HIDDEN_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));
        tensors.push(make_tensor(
            "model.layers.0.self_attn.k_proj.weight",
            vec![HIDDEN_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));
        tensors.push(make_tensor(
            "model.layers.0.self_attn.v_proj.weight",
            vec![HIDDEN_SIZE, HIDDEN_SIZE],
            &mut rng,
        ));

        serialize_tensors(tensors)
    }

    /// Create a tiny LoRA adapter with deterministic weights for all target modules.
    fn create_dummy_adapter() -> Result<Vec<u8>> {
        let mut rng = StdRng::seed_from_u64(4242);

        let modules = [
            ("self_attn.q_proj", HIDDEN_SIZE, HIDDEN_SIZE),
            ("self_attn.k_proj", HIDDEN_SIZE, HIDDEN_SIZE),
            ("self_attn.v_proj", HIDDEN_SIZE, HIDDEN_SIZE),
            ("mlp.down_proj", INTERMEDIATE_SIZE, HIDDEN_SIZE),
            ("mlp.up_proj", HIDDEN_SIZE, INTERMEDIATE_SIZE),
        ];

        let mut tensors = Vec::new();

        for (module, a_in_dim, b_out_dim) in modules {
            tensors.push(make_tensor(
                format!("base_model.model.layers.0.{module}.lora_A.weight"),
                vec![LORA_RANK, a_in_dim],
                &mut rng,
            ));
            tensors.push(make_tensor(
                format!("base_model.model.layers.0.{module}.lora_B.weight"),
                vec![b_out_dim, LORA_RANK],
                &mut rng,
            ));
        }

        serialize_tensors(tensors)
    }

    use std::io::Write;

    pub fn run() -> Result<()> {
        println!("Initializing Metal Kernel Harness...");
        std::io::stdout().flush().ok();

        // Initialize kernels and align GQA config with our dummy plan
        let mut kernels = MetalKernels::new()?;
        let gqa_config =
            GqaConfig::try_from_params(NUM_ATTENTION_HEADS, NUM_KV_HEADS, HIDDEN_SIZE, 10_000.0)?;
        kernels.set_gqa_config(gqa_config);

        let plan = create_dummy_plan()?;
        println!(
            "Dummy plan ready (vocab={}, hidden={}, intermediate={})",
            VOCAB_SIZE, HIDDEN_SIZE, INTERMEDIATE_SIZE
        );
        kernels.load(&plan)?;
        println!("Model loaded.");

        let adapter = create_dummy_adapter()?;
        kernels.load_adapter(ADAPTER_ID, &adapter)?;
        println!("Adapter loaded (id={}).", ADAPTER_ID);

        let mut ring = RouterRing::new(1);
        ring.set(&[ADAPTER_ID], &[RingBuffer::float_to_q15(1.0)]);

        let mut io = IoBuffers::new(VOCAB_SIZE);
        io.input_ids = vec![0]; // Single-token step to match buffer sizing

        println!("Starting inference loop...");
        std::io::stdout().flush().ok();
        for step in 0..10 {
            print!("  Processing step {}... ", step);
            std::io::stdout().flush().ok();

            ring.position = step;
            io.position = step;
            io.input_ids[0] = (step % VOCAB_SIZE) as u32;

            kernels.run_step(&ring, &mut io)?;

            let logit0 = io.output_logits.first().copied().unwrap_or_default();
            println!("Done (logit[0]={:.4}).", logit0);
            std::io::stdout().flush().ok();
        }

        println!("Harness finished successfully.");
        Ok(())
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    if let Err(e) = harness::run() {
        eprintln!("Harness failed: {}", e);
        std::process::exit(1);
    }

    #[cfg(not(target_os = "macos"))]
    println!("This harness requires macOS/Metal.");
}
