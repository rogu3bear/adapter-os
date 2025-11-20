//! Streaming inference example for MLX backend
//!
//! Demonstrates real-time token-by-token generation with:
//! - SSE formatting for HTTP streaming
//! - UTF-8 token healing
//! - Stop sequence detection
//! - Latency measurements
//!
//! Usage:
//! ```bash
//! cargo run --example streaming_inference --features experimental-backends
//! ```

use adapteros_core::B3Hash;
use adapteros_lora_mlx_ffi::streaming::{
    FinishReason, MLXStreamingGenerator, SSEFormatter, StreamEvent, StreamingConfig,
};
use futures::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 MLX Streaming Inference Example\n");

    // Configure streaming
    let config = StreamingConfig {
        max_tokens: 50,
        temperature: 0.7,
        stop_sequences: vec!["</s>".to_string(), "\n\n".to_string()],
        keep_alive: true,
        keep_alive_interval: Duration::from_secs(5),
        enable_utf8_healing: true,
        ..Default::default()
    };

    // Create streaming generator with deterministic seed
    let base_seed = B3Hash::hash(b"example-streaming-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 32);

    let (tx, mut rx) = mpsc::channel(100);

    println!("📝 Prompt: \"Explain quantum computing in simple terms:\"\n");
    println!("--- Streaming Response ---\n");

    // Mock token generation (in production, this would call the model)
    let tokens = vec![
        "Quantum",
        " computing",
        " uses",
        " the",
        " principles",
        " of",
        " quantum",
        " mechanics",
        " to",
        " process",
        " information",
        ".",
        " Unlike",
        " classical",
        " computers",
        " that",
        " use",
        " bits",
        " (0",
        " or",
        " 1",
        "),",
        " quantum",
        " computers",
        " use",
        " qubits",
        " that",
        " can",
        " exist",
        " in",
        " multiple",
        " states",
        " simultaneously",
        ".",
        " This",
        " allows",
        " them",
        " to",
        " solve",
        " certain",
        " problems",
        " much",
        " faster",
        " than",
        " traditional",
        " computers",
        ".",
        "</s>",
    ];

    let mut token_idx = 0;
    let generate_fn = move |step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        if token_idx >= tokens.len() {
            return Err(adapteros_core::AosError::Internal(
                "No more tokens".to_string(),
            ));
        }

        let token_text = tokens[token_idx];
        token_idx += 1;

        // Simulate processing delay
        std::thread::sleep(Duration::from_millis(50));

        Ok((step as u32, token_text.as_bytes().to_vec()))
    };

    // Start streaming generation
    let start_time = Instant::now();
    let gen_handle = tokio::spawn(async move {
        generator.generate(generate_fn, tx).await
    });

    // Consume and display stream
    let mut token_count = 0;
    let mut first_token_latency = None;
    let mut output = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token {
                text,
                delta_us,
                elapsed_us,
                ..
            } => {
                if first_token_latency.is_none() {
                    first_token_latency = Some(delta_us);
                    println!("\n⏱️  First token latency: {:.2}ms\n", delta_us as f64 / 1000.0);
                }

                // Print token in real-time
                print!("{}", text);
                use std::io::Write;
                std::io::stdout().flush()?;

                output.push_str(&text);
                token_count += 1;

                // Show latency every 10 tokens
                if token_count % 10 == 0 {
                    println!(
                        "\n[Token #{}: Δ={:.2}ms, Total={:.2}ms]",
                        token_count,
                        delta_us as f64 / 1000.0,
                        elapsed_us as f64 / 1000.0
                    );
                }
            }
            StreamEvent::Done {
                finish_reason,
                total_tokens,
                total_time_us,
                tokens_per_sec,
            } => {
                println!("\n\n--- Generation Complete ---\n");
                println!("✅ Finish reason: {:?}", finish_reason);
                println!("📊 Total tokens: {}", total_tokens);
                println!("⏱️  Total time: {:.2}ms", total_time_us as f64 / 1000.0);
                println!("🚀 Throughput: {:.2} tokens/sec", tokens_per_sec);

                if let Some(latency) = first_token_latency {
                    println!(
                        "⚡ First token latency: {:.2}ms",
                        latency as f64 / 1000.0
                    );
                }

                break;
            }
            StreamEvent::Error { message, code } => {
                eprintln!("\n❌ Error: {} (code: {})", message, code);
                break;
            }
            StreamEvent::KeepAlive => {
                // Keep-alive received
                continue;
            }
        }
    }

    gen_handle.await??;

    // Demonstrate SSE formatting
    println!("\n\n--- SSE Format Example ---\n");
    println!("Token event:");
    let token_event = StreamEvent::Token {
        text: "Hello".to_string(),
        token_id: 42,
        delta_us: 1500,
        elapsed_us: 5000,
    };
    print!("{}", SSEFormatter::format(&token_event));

    println!("\nDone event:");
    let done_event = StreamEvent::Done {
        finish_reason: FinishReason::Stop,
        total_tokens: 50,
        total_time_us: 250000,
        tokens_per_sec: 200.0,
    };
    print!("{}", SSEFormatter::format(&done_event));

    // Demonstrate UTF-8 healing
    println!("\n--- UTF-8 Token Healing Example ---\n");
    use adapteros_lora_mlx_ffi::streaming::UTF8TokenHealer;

    let mut healer = UTF8TokenHealer::new(true);

    println!("Processing split emoji (👍):");
    println!("  Bytes: [0xF0, 0x9F, 0x91, 0x8D]");

    let result1 = healer.process(&[0xF0, 0x9F]).unwrap();
    println!("  After [0xF0, 0x9F]: {:?} (buffered)", result1);

    let result2 = healer.process(&[0x91, 0x8D]).unwrap();
    println!("  After [0x91, 0x8D]: {:?} ✅", result2);

    // Demonstrate stop sequence detection
    println!("\n--- Stop Sequence Detection Example ---\n");
    use adapteros_lora_mlx_ffi::streaming::StopSequenceDetector;

    let mut detector = StopSequenceDetector::new(vec!["</s>".to_string(), "\n\n".to_string()]);

    let test_tokens = vec!["Hello ", "world", "!", " <", "/", "s", ">"];
    println!("Tokens: {:?}", test_tokens);

    for (i, token) in test_tokens.iter().enumerate() {
        let detected = detector.check(token);
        println!("  Token {}: '{}' -> stopped: {}", i + 1, token, detected);
        if detected {
            println!("  🛑 Stop sequence detected!");
            break;
        }
    }

    println!("\n✨ Example complete!\n");

    Ok(())
}

/// Example of using streaming with futures Stream trait
#[allow(dead_code)]
async fn stream_with_futures_example() -> Result<(), Box<dyn std::error::Error>> {
    use adapteros_lora_mlx_ffi::streaming::TokenStream;

    let (tx, rx) = mpsc::channel(100);
    let mut stream = TokenStream::new(rx);

    // Send some mock events
    tokio::spawn(async move {
        for i in 0..10 {
            let _ = tx
                .send(StreamEvent::Token {
                    text: format!("token{} ", i),
                    token_id: i,
                    delta_us: 1000,
                    elapsed_us: i as u64 * 1000,
                })
                .await;
        }

        let _ = tx
            .send(StreamEvent::Done {
                finish_reason: FinishReason::Length,
                total_tokens: 10,
                total_time_us: 10000,
                tokens_per_sec: 1000.0,
            })
            .await;
    });

    // Consume stream using futures Stream trait
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Token { text, .. } => print!("{}", text),
            StreamEvent::Done { .. } => {
                println!("\n✅ Done");
                break;
            }
            _ => continue,
        }
    }

    Ok(())
}
