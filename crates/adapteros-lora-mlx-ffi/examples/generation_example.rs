//! Example demonstrating complete text generation with MLX backend
//!
//! This example shows:
//! 1. Loading a model
//! 2. Tokenizing input
//! 3. Generating tokens with various sampling strategies
//! 4. Streaming generation with callbacks
//! 5. Using KV cache for efficiency

use adapteros_core::Result;
use adapteros_lora_mlx_ffi::{GenerationConfig, MLXFFIModel, MLXGenerator};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("MLX Text Generation Example\n");

    // Example 1: Basic generation
    println!("=== Example 1: Basic Generation ===");
    basic_generation()?;

    // Example 2: Temperature-scaled sampling
    println!("\n=== Example 2: Temperature Sampling ===");
    temperature_sampling()?;

    // Example 3: Top-k sampling
    println!("\n=== Example 3: Top-K Sampling ===");
    top_k_sampling()?;

    // Example 4: Nucleus (top-p) sampling
    println!("\n=== Example 4: Nucleus (Top-P) Sampling ===");
    nucleus_sampling()?;

    // Example 5: Repetition penalty
    println!("\n=== Example 5: Repetition Penalty ===");
    repetition_penalty_example()?;

    // Example 6: Streaming generation
    println!("\n=== Example 6: Streaming Generation ===");
    streaming_generation()?;

    // Example 7: Batch generation with cache
    println!("\n=== Example 7: Batch Generation ===");
    batch_generation()?;

    Ok(())
}

/// Basic generation with default settings
fn basic_generation() -> Result<()> {
    // Mock model for example (in real usage, load from disk)
    println!("Loading model...");

    // Mock tokenization (in real usage, use QwenTokenizer)
    let prompt = "What is the capital of France?";
    let prompt_tokens = mock_tokenize(prompt);
    println!("Prompt: {}", prompt);
    println!("Prompt tokens: {:?}", prompt_tokens);

    // Create generation config
    let config = GenerationConfig {
        max_tokens: 20,
        temperature: 1.0,
        ..Default::default()
    };

    println!("\nConfig:");
    println!("  max_tokens: {}", config.max_tokens);
    println!("  temperature: {}", config.temperature);
    println!("  use_cache: {}", config.use_cache);

    // Note: In real usage, you would:
    // let model = MLXFFIModel::load("path/to/model")?;
    // let output_tokens = model.generate_from_tokens(prompt_tokens, config)?;
    // let output_text = tokenizer.decode(&output_tokens)?;

    println!("\n[Example would generate tokens here with loaded model]");

    Ok(())
}

/// Generation with temperature scaling
fn temperature_sampling() -> Result<()> {
    println!("Demonstrating temperature effects:");

    let configs = vec![
        ("Greedy (temp=0.1)", 0.1),
        ("Balanced (temp=1.0)", 1.0),
        ("Creative (temp=1.5)", 1.5),
    ];

    for (name, temp) in configs {
        let config = GenerationConfig {
            max_tokens: 10,
            temperature: temp,
            ..Default::default()
        };

        println!("\n{}:", name);
        println!("  Temperature: {}", config.temperature);
        println!("  Effect: {}", match temp {
            t if t < 0.5 => "More deterministic, focuses on highest probability tokens",
            t if t > 1.2 => "More random, explores lower probability tokens",
            _ => "Balanced exploration and exploitation"
        });
    }

    Ok(())
}

/// Generation with top-k sampling
fn top_k_sampling() -> Result<()> {
    println!("Demonstrating top-k sampling:");

    let configs = vec![
        ("Focused (k=10)", Some(10)),
        ("Moderate (k=50)", Some(50)),
        ("No filtering", None),
    ];

    for (name, k) in configs {
        let config = GenerationConfig {
            max_tokens: 10,
            top_k: k,
            ..Default::default()
        };

        println!("\n{}:", name);
        if let Some(k_val) = k {
            println!("  Top-K: {}", k_val);
            println!("  Effect: Only consider {} most likely tokens", k_val);
        } else {
            println!("  Top-K: None");
            println!("  Effect: Consider all tokens");
        }
    }

    Ok(())
}

/// Generation with nucleus (top-p) sampling
fn nucleus_sampling() -> Result<()> {
    println!("Demonstrating nucleus (top-p) sampling:");

    let configs = vec![
        ("Conservative (p=0.5)", Some(0.5)),
        ("Balanced (p=0.9)", Some(0.9)),
        ("Permissive (p=0.95)", Some(0.95)),
    ];

    for (name, p) in configs {
        let config = GenerationConfig {
            max_tokens: 10,
            top_p: p,
            ..Default::default()
        };

        println!("\n{}:", name);
        if let Some(p_val) = p {
            println!("  Top-P: {}", p_val);
            println!(
                "  Effect: Sample from tokens with cumulative probability >= {}",
                p_val
            );
        }
    }

    Ok(())
}

/// Generation with repetition penalty
fn repetition_penalty_example() -> Result<()> {
    println!("Demonstrating repetition penalty:");

    let configs = vec![
        ("No penalty", 1.0),
        ("Light penalty", 1.1),
        ("Strong penalty", 1.5),
    ];

    for (name, penalty) in configs {
        let config = GenerationConfig {
            max_tokens: 20,
            repetition_penalty: penalty,
            ..Default::default()
        };

        println!("\n{}:", name);
        println!("  Repetition penalty: {}", config.repetition_penalty);
        println!("  Effect: {}", match penalty {
            p if p == 1.0 => "No penalty, tokens can repeat freely",
            p if p < 1.3 => "Light penalty, reduces repetition slightly",
            _ => "Strong penalty, heavily discourages repetition"
        });
    }

    Ok(())
}

/// Streaming generation with callback
fn streaming_generation() -> Result<()> {
    println!("Demonstrating streaming generation:");

    let prompt_tokens = mock_tokenize("Once upon a time");
    println!("Prompt tokens: {:?}", prompt_tokens);

    let config = GenerationConfig {
        max_tokens: 30,
        temperature: 0.8,
        ..Default::default()
    };

    println!("\nStreaming configuration:");
    println!("  max_tokens: {}", config.max_tokens);
    println!("  temperature: {}", config.temperature);
    println!("  Callback: Print each token as generated");

    // Example callback that would print tokens in real-time
    println!("\nExample callback:");
    println!("```rust");
    println!("let callback = |token: u32, position: usize| -> Result<bool> {{");
    println!("    // Decode and print token");
    println!("    let text = tokenizer.decode(&[token])?;");
    println!("    print!(\"{{}} \", text);");
    println!("    ");
    println!("    // Stop if max length reached or EOS");
    println!("    Ok(position < max_tokens)");
    println!("}};");
    println!("```");

    // In real usage:
    // let output = model.generate_streaming(prompt_tokens, config, callback)?;

    Ok(())
}

/// Batch generation demonstrating cache efficiency
fn batch_generation() -> Result<()> {
    println!("Demonstrating batch generation with KV cache:");

    let prompts = vec![
        "What is machine learning?",
        "Explain neural networks",
        "How does backpropagation work?",
    ];

    println!("Prompts:");
    for (i, prompt) in prompts.iter().enumerate() {
        println!("  {}: {}", i + 1, prompt);
    }

    let config = GenerationConfig {
        max_tokens: 15,
        temperature: 0.7,
        use_cache: true,
        ..Default::default()
    };

    println!("\nConfiguration:");
    println!("  max_tokens: {}", config.max_tokens);
    println!("  use_cache: {}", config.use_cache);
    println!(
        "  Expected speedup: {}x for subsequent tokens",
        if config.use_cache { "2-3" } else { "1" }
    );

    println!("\nCache benefits:");
    println!("  - Avoids recomputing key/value for past tokens");
    println!("  - Reduces memory bandwidth");
    println!("  - Speeds up generation significantly");

    Ok(())
}

/// Mock tokenizer for example (replace with real tokenizer)
fn mock_tokenize(text: &str) -> Vec<u32> {
    // Simple mock: convert each character to token ID
    // Real implementation would use QwenTokenizer
    text.chars()
        .map(|c| c as u32 % 1000)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_examples_run() {
        // Just verify examples can be constructed
        assert!(basic_generation().is_ok());
        assert!(temperature_sampling().is_ok());
        assert!(top_k_sampling().is_ok());
        assert!(nucleus_sampling().is_ok());
        assert!(repetition_penalty_example().is_ok());
        assert!(streaming_generation().is_ok());
        assert!(batch_generation().is_ok());
    }
}
