//! Token-by-token streaming tests with latency measurements
//!
//! Tests streaming generation functionality including:
//! - UTF-8 token healing
//! - Stop sequence detection
//! - SSE formatting
//! - First-token latency
//! - Throughput measurements
//! - Client disconnect handling

use adapteros_core::B3Hash;
use adapteros_lora_mlx_ffi::streaming::*;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Test UTF-8 healing with partial character sequences
#[test]
fn test_utf8_healing_emoji() {
    let mut healer = UTF8TokenHealer::new(true);

    // Emoji (👍) is 4 bytes: F0 9F 91 8D
    // Split across multiple tokens
    let result1 = healer.process(&[0xF0, 0x9F]).unwrap();
    assert_eq!(result1, None, "Should buffer incomplete sequence");

    let result2 = healer.process(&[0x91]).unwrap();
    assert_eq!(result2, None, "Should continue buffering");

    let result3 = healer.process(&[0x8D]).unwrap();
    assert_eq!(result3, Some("👍".to_string()), "Should emit complete emoji");
}

#[test]
fn test_utf8_healing_mixed() {
    let mut healer = UTF8TokenHealer::new(true);

    // ASCII + partial UTF-8
    let result1 = healer.process(b"Hello ").unwrap();
    assert_eq!(result1, Some("Hello ".to_string()));

    // Start of UTF-8 sequence (é = C3 A9)
    let result2 = healer.process(&[0xC3]).unwrap();
    assert_eq!(result2, None, "Should buffer incomplete");

    // Complete sequence
    let result3 = healer.process(&[0xA9, b' ', b'w', b'o', b'r', b'l', b'd']).unwrap();
    assert_eq!(result3, Some("é world".to_string()));
}

#[test]
fn test_utf8_healing_invalid_flush() {
    let mut healer = UTF8TokenHealer::new(true);

    // Invalid UTF-8 sequence
    healer.process(&[0xFF, 0xFE]).unwrap();

    // Flush should use replacement characters
    let result = healer.flush().unwrap();
    assert!(result.is_some());
    assert_ne!(result.unwrap(), ""); // Should contain replacement characters
}

/// Test stop sequence detection
#[test]
fn test_stop_sequence_simple() {
    let mut detector = StopSequenceDetector::new(vec!["</s>".to_string()]);

    assert!(!detector.check("Hello "));
    assert!(!detector.check("world "));
    assert!(!detector.check("<"));
    assert!(!detector.check("/"));
    assert!(detector.check("s>"), "Should detect stop sequence");
}

#[test]
fn test_stop_sequence_multiple() {
    let mut detector = StopSequenceDetector::new(vec![
        "</s>".to_string(),
        "\n\n".to_string(),
        "END".to_string(),
    ]);

    assert!(!detector.check("Starting text\n"));
    assert!(detector.check("\nDouble newline"), "Should detect \\n\\n");

    // Reset and test different sequence
    detector = StopSequenceDetector::new(vec!["END".to_string()]);
    assert!(!detector.check("The "));
    assert!(detector.check("END"), "Should detect END");
}

#[test]
fn test_stop_sequence_across_boundaries() {
    let mut detector = StopSequenceDetector::new(vec!["<|im_end|>".to_string()]);

    assert!(!detector.check("<|im_"));
    assert!(detector.check("end|>"), "Should detect across token boundaries");
}

/// Test SSE formatting
#[test]
fn test_sse_format_token() {
    let event = StreamEvent::Token {
        text: "Hello".to_string(),
        token_id: 42,
        delta_us: 1000,
        elapsed_us: 5000,
    };

    let sse = SSEFormatter::format(&event);
    assert!(sse.starts_with("data: "));
    assert!(sse.contains("Hello"));
    assert!(sse.contains("chat.completion.chunk"));
    assert!(sse.ends_with("\n\n"));
}

#[test]
fn test_sse_format_done() {
    let event = StreamEvent::Done {
        finish_reason: FinishReason::Stop,
        total_tokens: 100,
        total_time_us: 500000,
        tokens_per_sec: 200.0,
    };

    let sse = SSEFormatter::format(&event);
    assert!(sse.contains("data: [DONE]"));
    assert!(sse.contains("finish_reason"));
    assert!(sse.contains("stop"));
}

#[test]
fn test_sse_format_error() {
    let event = StreamEvent::Error {
        message: "Generation failed".to_string(),
        code: "error_code".to_string(),
    };

    let sse = SSEFormatter::format(&event);
    assert!(sse.contains("error"));
    assert!(sse.contains("Generation failed"));
}

#[test]
fn test_sse_format_keepalive() {
    let event = StreamEvent::KeepAlive;
    let sse = SSEFormatter::format(&event);
    assert_eq!(sse, ": keep-alive\n\n");
}

/// Test KV cache management
#[test]
fn test_kv_cache_basic() {
    let mut cache = KVCacheManager::new(4, 128);

    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());

    // Add cache for layer 0
    cache.update(0, vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]);
    assert_eq!(cache.len(), 1);
    assert!(!cache.is_empty());

    // Verify cached values
    let keys = cache.get_keys(0).unwrap();
    assert_eq!(keys, &[1.0, 2.0, 3.0]);

    let values = cache.get_values(0).unwrap();
    assert_eq!(values, &[4.0, 5.0, 6.0]);
}

#[test]
fn test_kv_cache_multiple_layers() {
    let mut cache = KVCacheManager::new(4, 128);

    // Add different data to different layers
    for layer in 0..4 {
        let keys = vec![layer as f32; 10];
        let values = vec![layer as f32 * 2.0; 10];
        cache.update(layer, keys, values);
    }

    assert_eq!(cache.len(), 4);

    // Verify each layer
    for layer in 0..4 {
        let keys = cache.get_keys(layer).unwrap();
        assert_eq!(keys[0], layer as f32);

        let values = cache.get_values(layer).unwrap();
        assert_eq!(values[0], layer as f32 * 2.0);
    }
}

#[test]
fn test_kv_cache_clear() {
    let mut cache = KVCacheManager::new(2, 128);

    cache.update(0, vec![1.0], vec![2.0]);
    cache.update(1, vec![3.0], vec![4.0]);

    assert_eq!(cache.len(), 2);

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());
}

/// Test streaming configuration
#[test]
fn test_streaming_config_defaults() {
    let config = StreamingConfig::default();

    assert_eq!(config.max_tokens, 512);
    assert_eq!(config.temperature, 0.7);
    assert!(config.keep_alive);
    assert_eq!(config.channel_buffer, 100);
    assert!(config.enable_utf8_healing);
}

#[test]
fn test_streaming_config_custom() {
    let config = StreamingConfig {
        max_tokens: 1024,
        temperature: 0.9,
        top_p: Some(0.95),
        stop_sequences: vec!["STOP".to_string()],
        keep_alive: false,
        keep_alive_interval: Duration::from_secs(30),
        channel_buffer: 200,
        token_timeout: Duration::from_secs(60),
        enable_utf8_healing: false,
    };

    assert_eq!(config.max_tokens, 1024);
    assert_eq!(config.temperature, 0.9);
    assert_eq!(config.top_p, Some(0.95));
    assert_eq!(config.stop_sequences.len(), 1);
    assert!(!config.keep_alive);
    assert_eq!(config.channel_buffer, 200);
    assert!(!config.enable_utf8_healing);
}

/// Test token stream async iteration
#[tokio::test]
async fn test_token_stream_basic() {
    let (tx, rx) = mpsc::channel(10);
    let mut stream = TokenStream::new(rx);

    // Send some events
    tokio::spawn(async move {
        tx.send(StreamEvent::Token {
            text: "Hello".to_string(),
            token_id: 1,
            delta_us: 100,
            elapsed_us: 100,
        })
        .await
        .unwrap();

        tx.send(StreamEvent::Token {
            text: " world".to_string(),
            token_id: 2,
            delta_us: 150,
            elapsed_us: 250,
        })
        .await
        .unwrap();

        tx.send(StreamEvent::Done {
            finish_reason: FinishReason::Stop,
            total_tokens: 2,
            total_time_us: 250,
            tokens_per_sec: 8000.0,
        })
        .await
        .unwrap();
    });

    // Consume events using Stream trait
    use futures::StreamExt;
    let events: Vec<StreamEvent> = stream.collect().await;

    assert_eq!(events.len(), 3);

    match &events[0] {
        StreamEvent::Token { text, .. } => assert_eq!(text, "Hello"),
        _ => panic!("Expected token event"),
    }

    match &events[1] {
        StreamEvent::Token { text, .. } => assert_eq!(text, " world"),
        _ => panic!("Expected token event"),
    }

    match &events[2] {
        StreamEvent::Done { finish_reason, .. } => {
            assert_eq!(*finish_reason, FinishReason::Stop)
        }
        _ => panic!("Expected done event"),
    }
}

/// Test streaming generator with mock token generation
#[tokio::test]
async fn test_streaming_generator_basic() {
    let config = StreamingConfig {
        max_tokens: 5,
        temperature: 0.7,
        stop_sequences: vec![],
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"test-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(100);

    // Mock token generation function
    let mut token_counter = 0u32;
    let generate_fn = move |_step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        token_counter += 1;
        let text = format!("token{} ", token_counter);
        Ok((token_counter, text.into_bytes()))
    };

    // Run generation in background
    let gen_handle = tokio::spawn(async move {
        generator.generate(generate_fn, tx).await
    });

    // Collect events
    let mut tokens = Vec::new();
    let mut done_reason = None;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token { text, .. } => tokens.push(text),
            StreamEvent::Done { finish_reason, .. } => {
                done_reason = Some(finish_reason);
                break;
            }
            StreamEvent::KeepAlive => continue,
            StreamEvent::Error { .. } => panic!("Unexpected error"),
        }
    }

    gen_handle.await.unwrap().unwrap();

    assert_eq!(tokens.len(), 5, "Should generate max_tokens");
    assert_eq!(done_reason, Some(FinishReason::Length));
}

/// Test streaming generator with stop sequence
#[tokio::test]
async fn test_streaming_generator_stop_sequence() {
    let config = StreamingConfig {
        max_tokens: 100,
        stop_sequences: vec!["STOP".to_string()],
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"test-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(100);

    // Mock generation that produces stop sequence
    let mut step_counter = 0;
    let generate_fn = move |_step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        step_counter += 1;
        let text = if step_counter == 3 {
            "STOP".to_string()
        } else {
            format!("token{} ", step_counter)
        };
        Ok((step_counter as u32, text.into_bytes()))
    };

    // Run generation
    let gen_handle = tokio::spawn(async move {
        generator.generate(generate_fn, tx).await
    });

    // Collect events
    let mut tokens = Vec::new();
    let mut done_reason = None;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token { text, .. } => tokens.push(text),
            StreamEvent::Done { finish_reason, .. } => {
                done_reason = Some(finish_reason);
                break;
            }
            StreamEvent::KeepAlive => continue,
            StreamEvent::Error { .. } => panic!("Unexpected error"),
        }
    }

    gen_handle.await.unwrap().unwrap();

    assert_eq!(tokens.len(), 3, "Should stop at stop sequence");
    assert_eq!(done_reason, Some(FinishReason::Stop));
    assert!(tokens[2].contains("STOP"));
}

/// Benchmark first-token latency
#[tokio::test]
async fn benchmark_first_token_latency() {
    let config = StreamingConfig {
        max_tokens: 10,
        enable_utf8_healing: true,
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"benchmark-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(100);

    let generate_fn = |_step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        // Simulate small processing delay
        std::thread::sleep(Duration::from_micros(100));
        Ok((1, b"token ".to_vec()))
    };

    let start = Instant::now();

    // Start generation
    tokio::spawn(async move {
        let _ = generator.generate(generate_fn, tx).await;
    });

    // Measure time to first token
    let first_token_time = if let Some(StreamEvent::Token { .. }) = rx.recv().await {
        start.elapsed()
    } else {
        panic!("No token received");
    };

    println!("First token latency: {:?}", first_token_time);
    assert!(
        first_token_time < Duration::from_millis(10),
        "First token should arrive quickly"
    );
}

/// Benchmark throughput (tokens per second)
#[tokio::test]
async fn benchmark_token_throughput() {
    let token_count = 100;
    let config = StreamingConfig {
        max_tokens: token_count,
        enable_utf8_healing: false, // Disable for pure throughput test
        keep_alive: false,
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"throughput-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(200);

    let generate_fn = |_step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        Ok((1, b"t ".to_vec()))
    };

    let start = Instant::now();

    // Start generation
    tokio::spawn(async move {
        let _ = generator.generate(generate_fn, tx).await;
    });

    // Count tokens
    let mut count = 0;
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token { .. } => count += 1,
            StreamEvent::Done { .. } => break,
            _ => continue,
        }
    }

    let elapsed = start.elapsed();
    let tokens_per_sec = count as f32 / elapsed.as_secs_f32();

    println!(
        "Throughput: {:.0} tokens/sec ({} tokens in {:?})",
        tokens_per_sec, count, elapsed
    );

    assert_eq!(count, token_count, "Should generate all tokens");
    assert!(tokens_per_sec > 1000.0, "Should achieve high throughput");
}

/// Test client disconnect handling
#[tokio::test]
async fn test_client_disconnect() {
    let config = StreamingConfig {
        max_tokens: 1000,
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"disconnect-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(10);

    let generate_fn = |_step: usize, _seed: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        Ok((1, b"token ".to_vec()))
    };

    // Start generation
    tokio::spawn(async move {
        let _ = generator.generate(generate_fn, tx).await;
    });

    // Receive a few tokens then drop receiver (simulate disconnect)
    for _ in 0..3 {
        rx.recv().await;
    }

    drop(rx); // Client disconnect

    // Generation should complete without panic
    tokio::time::sleep(Duration::from_millis(100)).await;
}

/// Test streaming with UTF-8 healing across token boundaries
#[tokio::test]
async fn test_streaming_utf8_healing_integration() {
    let config = StreamingConfig {
        max_tokens: 10,
        enable_utf8_healing: true,
        ..Default::default()
    };

    let base_seed = B3Hash::hash(b"utf8-seed");
    let mut generator = MLXStreamingGenerator::new(config, base_seed, 4);

    let (tx, mut rx) = mpsc::channel(100);

    // Generate tokens with split UTF-8 sequences
    let mut step = 0;
    let generate_fn = move |_: usize, _: &B3Hash| -> adapteros_core::Result<(u32, Vec<u8>)> {
        step += 1;
        let bytes = match step {
            1 => b"Hello ".to_vec(),
            2 => vec![0xF0, 0x9F], // Start of 👍
            3 => vec![0x91, 0x8D], // End of 👍
            4 => b" world".to_vec(),
            _ => b" end".to_vec(),
        };
        Ok((step, bytes))
    };

    tokio::spawn(async move {
        let _ = generator.generate(generate_fn, tx).await;
    });

    // Collect text
    let mut text = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token { text: token_text, .. } => text.push_str(&token_text),
            StreamEvent::Done { .. } => break,
            _ => continue,
        }
    }

    println!("Generated text: {}", text);
    assert!(text.contains("👍"), "Should properly reconstruct emoji");
    assert!(text.contains("Hello"), "Should contain Hello");
    assert!(text.contains("world"), "Should contain world");
}
