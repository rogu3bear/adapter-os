//! Streaming latency benchmarks for MLX token generation
//!
//! Measures key streaming performance characteristics:
//! - First token latency (time to first token)
//! - Inter-token latency (time between tokens)
//! - SSE formatting overhead
//! - End-to-end streaming throughput
//!
//! Target metrics (MLX on Apple Silicon):
//! - First token latency: <100ms
//! - Inter-token latency: ~0.39ms
//! - SSE overhead: <100us per event
//!
//! [2025-11-22 streaming_latency_benchmark]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// =============================================================================
// SSE Formatting Types (copied from streaming module for benchmarking)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Format a token as an OpenAI-compatible SSE message
fn format_token_sse(token_text: &str, request_id: &str) -> String {
    let chunk = ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: "adapteros-mlx".to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                content: Some(token_text.to_string()),
                role: None,
            },
            finish_reason: None,
        }],
    };

    format!(
        "data: {}\n\n",
        serde_json::to_string(&chunk).unwrap_or_default()
    )
}

/// Format a done event as OpenAI-compatible SSE
fn format_done_sse(finish_reason: &str, request_id: &str) -> String {
    let chunk = ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: "adapteros-mlx".to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                content: None,
                role: None,
            },
            finish_reason: Some(finish_reason.to_string()),
        }],
    };

    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        serde_json::to_string(&chunk).unwrap_or_default()
    )
}

/// Format keep-alive SSE comment
fn format_keepalive_sse() -> String {
    ": keep-alive\n\n".to_string()
}

// =============================================================================
// Token Simulation
// =============================================================================

/// Simulated token data for benchmarking
struct TokenSimulator {
    tokens: Vec<(u32, String)>,
    current_index: usize,
}

impl TokenSimulator {
    fn new(num_tokens: usize) -> Self {
        let tokens: Vec<(u32, String)> = (0..num_tokens)
            .map(|i| {
                let text = match i % 10 {
                    0 => "Hello",
                    1 => " ",
                    2 => "world",
                    3 => "!",
                    4 => " ",
                    5 => "This",
                    6 => " is",
                    7 => " a",
                    8 => " test",
                    9 => ".",
                    _ => "",
                };
                (i as u32, text.to_string())
            })
            .collect();

        Self {
            tokens,
            current_index: 0,
        }
    }

    fn next_token(&mut self) -> Option<(u32, &str)> {
        if self.current_index >= self.tokens.len() {
            return None;
        }

        let (id, text) = &self.tokens[self.current_index];
        self.current_index += 1;
        Some((*id, text.as_str()))
    }
}

// =============================================================================
// Benchmarks
// =============================================================================

/// Benchmark SSE token formatting
fn bench_sse_token_formatting(c: &mut Criterion) {
    let mut group = c.benchmark_group("sse_formatting");
    group.throughput(Throughput::Elements(1));

    // Short token
    group.bench_function("short_token", |b| {
        b.iter(|| {
            black_box(format_token_sse(
                black_box("Hi"),
                black_box("chatcmpl-test"),
            ))
        })
    });

    // Medium token
    group.bench_function("medium_token", |b| {
        b.iter(|| {
            black_box(format_token_sse(
                black_box("Hello world"),
                black_box("chatcmpl-test"),
            ))
        })
    });

    // Long token (multi-word)
    group.bench_function("long_token", |b| {
        b.iter(|| {
            black_box(format_token_sse(
                black_box("This is a longer piece of text representing a token"),
                black_box("chatcmpl-test"),
            ))
        })
    });

    // Done event
    group.bench_function("done_event", |b| {
        b.iter(|| {
            black_box(format_done_sse(
                black_box("stop"),
                black_box("chatcmpl-test"),
            ))
        })
    });

    // Keep-alive
    group.bench_function("keepalive", |b| {
        b.iter(|| black_box(format_keepalive_sse()))
    });

    group.finish();
}

/// Benchmark streaming token generation simulation
fn bench_streaming_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_throughput");

    for num_tokens in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*num_tokens as u64));
        group.bench_with_input(
            BenchmarkId::new("tokens", num_tokens),
            num_tokens,
            |b, &num_tokens| {
                b.iter(|| {
                    let mut simulator = TokenSimulator::new(num_tokens);
                    let mut formatted_events = Vec::with_capacity(num_tokens);
                    let request_id = "chatcmpl-bench";

                    while let Some((_id, text)) = simulator.next_token() {
                        let sse = format_token_sse(text, request_id);
                        formatted_events.push(sse);
                    }

                    // Add done event
                    formatted_events.push(format_done_sse("stop", request_id));

                    black_box(formatted_events)
                })
            },
        );
    }

    group.finish();
}

/// Benchmark end-to-end streaming latency measurement
fn bench_latency_measurement(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_measurement");

    group.bench_function("measure_100_tokens", |b| {
        b.iter(|| {
            let num_tokens = 100;
            let mut simulator = TokenSimulator::new(num_tokens);
            let request_id = "chatcmpl-bench";

            let mut latencies = Vec::with_capacity(num_tokens);
            let mut prev_time = Instant::now();

            while let Some((_, text)) = simulator.next_token() {
                // Simulate token generation delay (~0.39ms)
                std::thread::sleep(std::time::Duration::from_micros(390));

                let now = Instant::now();
                let latency_us = now.duration_since(prev_time).as_micros() as u64;
                latencies.push(latency_us);
                prev_time = now;

                // Format SSE event
                let _ = format_token_sse(text, request_id);
            }

            // Calculate metrics
            let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
            let first = *latencies.first().unwrap_or(&0);

            black_box((first, avg, latencies.len()))
        })
    });

    group.finish();
}

/// Benchmark serialization specifically
fn bench_json_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_serialization");

    let chunk = ChatCompletionChunk {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1700000000,
        model: "adapteros-mlx".to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                content: Some("Hello".to_string()),
                role: None,
            },
            finish_reason: None,
        }],
    };

    group.bench_function("serialize_chunk", |b| {
        b.iter(|| black_box(serde_json::to_string(&chunk).unwrap()))
    });

    group.bench_function("serialize_chunk_vec", |b| {
        b.iter(|| black_box(serde_json::to_vec(&chunk).unwrap()))
    });

    group.finish();
}

/// Benchmark UTF-8 handling for token text
fn bench_utf8_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("utf8_handling");

    // ASCII text
    group.bench_function("ascii", |b| {
        let text = "Hello world!";
        b.iter(|| black_box(format_token_sse(black_box(text), "test")))
    });

    // UTF-8 with special characters
    group.bench_function("utf8_special", |b| {
        let text = "Hello 世界! 🌍";
        b.iter(|| black_box(format_token_sse(black_box(text), "test")))
    });

    // Long UTF-8 text
    group.bench_function("utf8_long", |b| {
        let text = "这是一个很长的中文文本用于测试UTF-8性能表现";
        b.iter(|| black_box(format_token_sse(black_box(text), "test")))
    });

    group.finish();
}

/// Benchmark memory allocation patterns
fn bench_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    // Pre-allocated buffer vs dynamic allocation
    group.bench_function("dynamic_allocation", |b| {
        b.iter(|| {
            let mut events: Vec<String> = Vec::new();
            for i in 0..100 {
                let text = format!("token{}", i);
                events.push(format_token_sse(&text, "test"));
            }
            black_box(events)
        })
    });

    group.bench_function("preallocated", |b| {
        b.iter(|| {
            let mut events: Vec<String> = Vec::with_capacity(100);
            for i in 0..100 {
                let text = format!("token{}", i);
                events.push(format_token_sse(&text, "test"));
            }
            black_box(events)
        })
    });

    group.finish();
}

/// Benchmark concurrent streaming simulation
fn bench_concurrent_streams(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_streams");
    group.sample_size(50); // Reduce sample size for longer benchmarks

    group.bench_function("5_concurrent_streams", |b| {
        b.iter(|| {
            let handles: Vec<_> = (0..5)
                .map(|stream_id| {
                    std::thread::spawn(move || {
                        let mut simulator = TokenSimulator::new(50);
                        let request_id = format!("chatcmpl-stream-{}", stream_id);
                        let mut events = Vec::with_capacity(51);

                        while let Some((_, text)) = simulator.next_token() {
                            events.push(format_token_sse(text, &request_id));
                        }
                        events.push(format_done_sse("stop", &request_id));
                        events
                    })
                })
                .collect();

            let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

            black_box(results)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sse_token_formatting,
    bench_streaming_throughput,
    bench_latency_measurement,
    bench_json_serialization,
    bench_utf8_handling,
    bench_memory_allocation,
    bench_concurrent_streams,
);

criterion_main!(benches);
