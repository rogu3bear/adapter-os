#![cfg(all(test, feature = "extended-tests"))]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use adapteros_benchmarks::*;
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// Benchmark evidence collection and processing
fn bench_evidence_collection(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark evidence score computation
        c.bench_function("evidence_score_computation", |b| {
            b.iter(|| {
                let mut rng = utils::DeterministicRng::new(42);
                let mut scores = Vec::new();

                for _ in 0..1000 {
                    // Simulate evidence scoring based on multiple factors
                    let relevance_score = rng.next_f32();
                    let confidence_score = rng.next_f32();
                    let recency_score = rng.next_f32();

                    // Combine scores with weights
                    let combined_score = 0.4 * relevance_score + 0.4 * confidence_score + 0.2 * recency_score;
                    scores.push(combined_score);
                }

                black_box(scores);
            })
        });

        // Benchmark evidence ranking
        c.bench_function("evidence_ranking_1000_items", |b| {
            b.iter(|| {
                let mut evidence_items: Vec<(String, f32)> = (0..1000)
                    .map(|i| (format!("evidence_{}", i), utils::DeterministicRng::new(i as u64).next_f32()))
                    .collect();

                // Rank by score (descending)
                evidence_items.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                let top_10: Vec<_> = evidence_items.into_iter().take(10).collect();
                black_box(top_10);
            })
        });

        // Benchmark evidence filtering
        c.bench_function("evidence_filtering_threshold", |b| {
            b.iter(|| {
                let threshold = 0.5f32;
                let mut rng = utils::DeterministicRng::new(42);
                let mut filtered_evidence = Vec::new();
                let mut rejected_count = 0;

                for i in 0..2000 {
                    let score = rng.next_f32();
                    let evidence = format!("evidence_{}", i);

                    if score >= threshold {
                        filtered_evidence.push((evidence, score));
                    } else {
                        rejected_count += 1;
                    }
                }

                black_box((filtered_evidence, rejected_count));
            })
        });
    });
}

/// Benchmark evidence-grounded response generation
fn bench_evidence_grounding(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark response grounding with evidence
        c.bench_function("response_grounding_10_evidence", |b| {
            b.iter(|| {
                let base_response = "This is a response based on evidence.";
                let evidence_list = vec![
                    "Evidence 1: Supporting fact A",
                    "Evidence 2: Supporting fact B",
                    "Evidence 3: Supporting fact C",
                    "Evidence 4: Supporting fact D",
                    "Evidence 5: Supporting fact E",
                    "Evidence 6: Supporting fact F",
                    "Evidence 7: Supporting fact G",
                    "Evidence 8: Supporting fact H",
                    "Evidence 9: Supporting fact I",
                    "Evidence 10: Supporting fact J",
                ];

                // Simulate grounding process
                let mut grounded_response = base_response.to_string();
                let mut citations = Vec::new();

                for (i, evidence) in evidence_list.iter().enumerate() {
                    if i < 3 { // Use top 3 evidence items
                        grounded_response.push_str(&format!(" [{}]", i + 1));
                        citations.push(evidence.clone());
                    }
                }

                black_box((grounded_response, citations));
            })
        });

        // Benchmark confidence scoring for grounded responses
        c.bench_function("confidence_scoring_grounded_response", |b| {
            b.iter(|| {
                let evidence_scores = vec![0.9, 0.8, 0.7, 0.6, 0.5];
                let evidence_weights = vec![0.3, 0.25, 0.2, 0.15, 0.1];

                // Calculate weighted confidence
                let mut total_weighted_score = 0.0f32;
                let mut total_weight = 0.0f32;

                for (score, weight) in evidence_scores.iter().zip(evidence_weights.iter()) {
                    total_weighted_score += score * weight;
                    total_weight += weight;
                }

                let confidence_score = if total_weight > 0.0 {
                    total_weighted_score / total_weight
                } else {
                    0.0
                };

                // Apply confidence threshold
                let is_confident = confidence_score >= 0.7;

                black_box((confidence_score, is_confident));
            })
        });

        // Benchmark evidence chain validation
        c.bench_function("evidence_chain_validation", |b| {
            b.iter(|| {
                let evidence_chain = vec![
                    ("source_1", 0.9, vec!["fact_a", "fact_b"]),
                    ("source_2", 0.8, vec!["fact_b", "fact_c"]),
                    ("source_3", 0.7, vec!["fact_c", "fact_d"]),
                    ("source_4", 0.6, vec!["fact_d", "fact_e"]),
                ];

                let mut validated_chain = Vec::new();
                let mut consistency_score = 1.0f32;

                for (source, score, facts) in &evidence_chain {
                    // Check internal consistency of facts
                    let internal_consistency = facts.len() as f32 / 10.0; // Simple heuristic

                    // Check consistency with previous evidence
                    if !validated_chain.is_empty() {
                        let prev_facts = &validated_chain.last().unwrap().2;
                        let overlap = facts.iter().filter(|f| prev_facts.contains(f)).count();
                        let overlap_ratio = overlap as f32 / facts.len().max(prev_facts.len()) as f32;
                        consistency_score *= overlap_ratio;
                    }

                    validated_chain.push((source.clone(), *score * internal_consistency, facts.clone()));
                }

                let is_valid_chain = consistency_score >= 0.5;
                black_box((validated_chain, consistency_score, is_valid_chain));
            })
        });
    });
}

/// Benchmark evidence caching and retrieval
fn bench_evidence_caching(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark evidence cache insertion
        c.bench_function("evidence_cache_insertion_1000", |b| {
            b.iter(|| {
                let mut cache = HashMap::new();

                for i in 0..1000 {
                    let key = format!("evidence_key_{}", i);
                    let evidence = format!("Evidence data for item {}", i);
                    let score = utils::DeterministicRng::new(i as u64).next_f32();

                    cache.insert(key, (evidence, score));
                }

                black_box(cache.len());
            })
        });

        // Benchmark evidence cache lookup
        c.bench_function("evidence_cache_lookup_1000", |b| {
            b.iter(|| {
                let mut cache = HashMap::new();

                // Populate cache
                for i in 0..1000 {
                    let key = format!("evidence_key_{}", i);
                    let evidence = format!("Evidence data for item {}", i);
                    let score = utils::DeterministicRng::new(i as u64).next_f32();
                    cache.insert(key, (evidence, score));
                }

                // Perform lookups
                let mut found_items = Vec::new();
                for i in (0..1000).step_by(10) { // Lookup every 10th item
                    let key = format!("evidence_key_{}", i);
                    if let Some((evidence, score)) = cache.get(&key) {
                        found_items.push((evidence.clone(), *score));
                    }
                }

                black_box(found_items);
            })
        });

        // Benchmark evidence cache eviction
        c.bench_function("evidence_cache_eviction_lru", |b| {
            b.iter(|| {
                let mut cache = HashMap::new();
                let mut access_order = Vec::new();
                let max_size = 100;

                // Simulate cache operations
                for i in 0..200 {
                    let key = format!("evidence_key_{}", i % 150); // Some keys repeat

                    if cache.contains_key(&key) {
                        // Update access order (move to front)
                        access_order.retain(|k| k != &key);
                        access_order.push(key.clone());
                    } else {
                        // New entry
                        if cache.len() >= max_size {
                            // Evict least recently used
                            if let Some(lru_key) = access_order.first().cloned() {
                                cache.remove(&lru_key);
                                access_order.remove(0);
                            }
                        }

                        let evidence = format!("Evidence data for item {}", i);
                        let score = utils::DeterministicRng::new(i as u64).next_f32();
                        cache.insert(key.clone(), (evidence, score));
                        access_order.push(key);
                    }
                }

                black_box((cache.len(), access_order.len()));
            })
        });
    });
}

/// Benchmark evidence-based decision making
fn bench_evidence_decisions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark multi-criteria evidence evaluation
        c.bench_function("multi_criteria_evidence_evaluation", |b| {
            b.iter(|| {
                let criteria = vec![
                    ("accuracy", 0.4),
                    ("relevance", 0.3),
                    ("timeliness", 0.2),
                    ("authority", 0.1),
                ];

                let evidence_sets = vec![
                    vec![0.9, 0.8, 0.7, 0.8], // Evidence set 1
                    vec![0.7, 0.9, 0.8, 0.6], // Evidence set 2
                    vec![0.8, 0.6, 0.9, 0.7], // Evidence set 3
                ];

                let mut evaluations = Vec::new();

                for evidence_set in &evidence_sets {
                    let mut total_score = 0.0f32;

                    for (i, &score) in evidence_set.iter().enumerate() {
                        let (_, weight) = criteria[i];
                        total_score += score * weight;
                    }

                    evaluations.push(total_score);
                }

                // Select best evidence set
                let best_index = evaluations.iter().enumerate()
                    .max_by(|a, b| {
                        a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap();

                black_box((evaluations, best_index));
            })
        });

        // Benchmark evidence consensus building
        c.bench_function("evidence_consensus_building", |b| {
            b.iter(|| {
                let num_experts = 5;
                let num_claims = 10;
                let mut expert_opinions = Vec::new();

                // Generate expert opinions
                for expert in 0..num_experts {
                    let mut opinions = Vec::new();
                    let mut rng = utils::DeterministicRng::new(expert as u64);

                    for _ in 0..num_claims {
                        opinions.push(rng.next_f32()); // Confidence score for claim
                    }

                    expert_opinions.push(opinions);
                }

                // Build consensus for each claim
                let mut consensus_scores = Vec::new();

                for claim_idx in 0..num_claims {
                    let mut scores = Vec::new();

                    for expert_opinions in &expert_opinions {
                        scores.push(expert_opinions[claim_idx]);
                    }

                    // Simple consensus: average of top 3 scores
                    scores.sort_by(|a, b| {
                        b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let top_3_sum: f32 = scores.iter().take(3).sum();
                    let consensus = top_3_sum / 3.0;

                    consensus_scores.push(consensus);
                }

                black_box(consensus_scores);
            })
        });

        // Benchmark evidence quality assessment
        c.bench_function("evidence_quality_assessment", |b| {
            b.iter(|| {
                let evidence_items = vec![
                    ("peer_reviewed_journal", 0.9, 100, 0.95), // source, credibility, citations, recency
                    ("social_media_post", 0.2, 5, 0.3),
                    ("government_report", 0.8, 50, 0.8),
                    ("blog_post", 0.3, 10, 0.6),
                    ("academic_thesis", 0.85, 75, 0.9),
                ];

                let mut quality_scores = Vec::new();

                for (source, credibility, citations, recency) in &evidence_items {
                    // Quality formula: weighted combination of factors
                    let citation_score = (*citations as f32 / 100.0).min(1.0); // Normalize citations
                    let quality = 0.4 * credibility + 0.3 * citation_score + 0.3 * recency;

                    quality_scores.push((source.to_string(), quality));
                }

                // Sort by quality
                quality_scores.sort_by(|a, b| {
                    b.1.partial_cmp(&a.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                black_box(quality_scores);
            })
        });
    });
}

/// Benchmark response latency with evidence processing
fn bench_response_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        // Benchmark end-to-end response time with evidence
        c.bench_function("end_to_end_response_with_evidence", |b| {
            b.iter_custom(|iters| {
                let start = Instant::now();

                for _ in 0..iters {
                    async fn process_with_evidence() -> Duration {
                        let request_start = Instant::now();

                        // Simulate evidence gathering
                        tokio::time::sleep(Duration::from_millis(10)).await;

                        // Simulate evidence processing
                        let mut evidence_scores = Vec::new();
                        for i in 0..50 {
                            let score = utils::DeterministicRng::new(i as u64).next_f32();
                            evidence_scores.push(score);
                        }

                        // Simulate response generation with evidence
                        tokio::time::sleep(Duration::from_millis(20)).await;

                        // Filter and rank evidence
                        evidence_scores.sort_by(|a, b| {
                            b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        let top_evidence: Vec<f32> = evidence_scores.into_iter().take(10).collect();

                        // Generate response
                        let response = format!("Response based on {} evidence items", top_evidence.len());
                        black_box((response, top_evidence));

                        request_start.elapsed()
                    }

                    let _latency = process_with_evidence().await;
                }

                start.elapsed()
            })
        });

        // Benchmark evidence processing pipeline latency
        c.bench_function("evidence_processing_pipeline_latency", |b| {
            b.iter_custom(|iters| {
                let start = Instant::now();

                for _ in 0..iters {
                    let pipeline_start = Instant::now();

                    // Stage 1: Evidence collection
                    let collection_start = Instant::now();
                    let mut raw_evidence = Vec::new();
                    for i in 0..100 {
                        raw_evidence.push(format!("raw_evidence_{}", i));
                    }
                    let collection_time = collection_start.elapsed();

                    // Stage 2: Evidence scoring
                    let scoring_start = Instant::now();
                    let mut scored_evidence = Vec::new();
                    for evidence in raw_evidence {
                        let score = utils::DeterministicRng::new(evidence.len() as u64).next_f32();
                        scored_evidence.push((evidence, score));
                    }
                    let scoring_time = scoring_start.elapsed();

                    // Stage 3: Evidence filtering and ranking
                    let filtering_start = Instant::now();
                    scored_evidence.sort_by(|a, b| {
                        b.1.partial_cmp(&a.1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let top_evidence: Vec<_> = scored_evidence.into_iter().take(20).collect();
                    let filtering_time = filtering_start.elapsed();

                    let total_pipeline_time = pipeline_start.elapsed();

                    black_box((collection_time, scoring_time, filtering_time, total_pipeline_time, top_evidence));
                }

                start.elapsed()
            })
        });
    });
}

criterion_group!(
    name = evidence_benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(20))
        .noise_threshold(0.05);
    targets = bench_evidence_collection, bench_evidence_grounding, bench_evidence_caching,
             bench_evidence_decisions, bench_response_latency
);

criterion_main!(evidence_benches);
