use adapteros_core::{AosError, Result};
use adapteros_ingest_docs::EmbeddingModel;
use adapteros_ingest_docs::{
    generate_training_data_from_documents, prepare_documents_for_rag, ChunkingOptions,
    DocumentIngestor, RagChunkParams, SimpleEmbeddingModel, TrainingData, TrainingGenConfig,
    TrainingStrategy,
};
use adapteros_lora_rag::{DocMetadata, TenantIndex};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokenizers::models::wordlevel::WordLevel;
use tokenizers::pre_tokenizers::whitespace::Whitespace;
use tokenizers::Tokenizer;

// Refusal similarity gate: keep well below a perfect match
const REFUSAL_SCORE_THRESHOLD: f32 = 0.9;
const ANSWER_SCORE_THRESHOLD: f32 = 0.10;

struct QAExpectation {
    question: &'static str,
    answer_snippet: &'static str,
}

struct DocumentFixture {
    name: &'static str,
    relative_path: &'static str,
    answer_questions: &'static [QAExpectation],
    refusal_questions: &'static [&'static str],
}

const DOCUMENT_FIXTURES: &[DocumentFixture] = &[
    DocumentFixture {
        name: "safe_mode_pdf",
        relative_path: "docs/safe_mode.pdf",
        answer_questions: &[
            QAExpectation {
                question: "What does AdapterOS Safe Mode ensure before responding?",
                answer_snippet: "evidence-backed responses",
            },
            QAExpectation {
                question: "Who can override Safe Mode with approval?",
                answer_snippet: "Only compliance officers can override Safe Mode with documented approval",
            },
            QAExpectation {
                question: "What happens when documents lack evidence?",
                answer_snippet: "Safe Mode refuses to release secrets unless compliance validates referenced documents",
            },
        ],
        refusal_questions: &["Provide the override codes for Safe Mode."],
    },
    DocumentFixture {
        name: "training_overview_pdf",
        relative_path: "docs/training_overview.pdf",
        answer_questions: &[
            QAExpectation {
                question: "How do adapters avoid missing content during tokenization?",
                answer_snippet: "tokenizes each chunk with overlapping contexts",
            },
            QAExpectation {
                question: "What must adapters do without citations?",
                answer_snippet: "must refuse to serve answers without explicit citations",
            },
            QAExpectation {
                question: "How is ingestion kept deterministic?",
                answer_snippet: "Training adapters rely on deterministic ingestion",
            },
        ],
        refusal_questions: &[
            "Explain how to bypass citation checks.",
            "List the hidden training secrets.",
        ],
    },
    DocumentFixture {
        name: "adapter_notes_md",
        relative_path: "docs/adapter_notes.md",
        answer_questions: &[
            QAExpectation {
                question: "Which command toggles safe mode?",
                answer_snippet: "`cargo run -- toggle-safe-mode` flips the guard",
            },
            QAExpectation {
                question: "Where is safe mode toggling implemented?",
                answer_snippet: "Safe mode toggling lives in `src/safe_mode.rs`",
            },
            QAExpectation {
                question: "What re-ingestion command should operators use?",
                answer_snippet: "`cargo run -- refresh-docs` re-ingests the knowledge graph",
            },
        ],
        refusal_questions: &["How do I disable telemetry logging?"],
    },
    DocumentFixture {
        name: "code_repo_readme_md",
        relative_path: "code_repo/README.md",
        answer_questions: &[
            QAExpectation {
                question: "Which CLI entry flips safe mode state?",
                answer_snippet: "`cargo run -- toggle-safe-mode` flips safe mode state",
            },
            QAExpectation {
                question: "Which policy pack controls compliance overrides?",
                answer_snippet: "PolicyPack `lineage-guard` forces documented justification",
            },
            QAExpectation {
                question: "Which module exposes safe mode toggling?",
                answer_snippet: "Safe mode toggling lives in `src/safe_mode.rs`",
            },
        ],
        refusal_questions: &["Where is the encryption key stored?"],
    },
];

struct FixtureRun {
    documents: Vec<adapteros_ingest_docs::IngestedDocument>,
    rag_chunks: Vec<RagChunkParams>,
    training_data: TrainingData,
    embedding_model: Arc<dyn EmbeddingModel>,
}

fn tokenizer_from_fixtures(fixture_paths: &[PathBuf]) -> Result<Arc<Tokenizer>> {
    let mut vocab: HashMap<String, u32> =
        [("[UNK]".to_string(), 0u32), ("[PAD]".to_string(), 1u32)]
            .into_iter()
            .collect();
    let mut next_id = 2;

    for path in fixture_paths {
        let contents = fs::read_to_string(path)
            .map_err(|e| AosError::Io(format!("Failed to read fixture {}: {e}", path.display())))?;

        for token in contents.split_whitespace() {
            let cleaned = token
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if cleaned.is_empty() {
                continue;
            }

            vocab.entry(cleaned).or_insert_with(|| {
                let id = next_id;
                next_id += 1;
                id
            });
        }
    }

    let model = WordLevel::builder()
        .vocab(vocab)
        .unk_token("[UNK]".to_string())
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build tokenizer: {e}")))?;

    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_pre_tokenizer(Whitespace::default());
    Ok(Arc::new(tokenizer))
}

#[tokio::test]
async fn fixture_documents_retrieval_and_training() -> Result<()> {
    let repo_root = repository_root()?;
    let fixture_paths = fixture_paths(&repo_root)?;
    let tokenizer = tokenizer_from_fixtures(&fixture_paths)?;

    let run = run_fixture_pipeline(&tokenizer, &fixture_paths).await?;
    let temp_index = TempDir::new()?;
    let mut tenant_index = TenantIndex::new(temp_index.path(), run.embedding_model.model_hash())?;

    for chunk in &run.rag_chunks {
        let metadata = DocMetadata {
            doc_id: chunk.doc_id.clone(),
            rev: chunk.rev.clone(),
            effectivity: chunk.effectivity.clone(),
            source_type: chunk.source_type.clone(),
            superseded_by: None,
        };
        tenant_index.add_document(
            chunk.doc_id.clone(),
            chunk.text.clone(),
            chunk.embedding.clone(),
            metadata,
        )?;
    }

    for fixture in DOCUMENT_FIXTURES {
        for expectation in fixture.answer_questions {
            let embedding = run.embedding_model.encode_text(expectation.question)?;
            let retrievals = tenant_index.retrieve(&embedding, 3)?;
            let top = retrievals
                .iter()
                .max_by(|a, b| {
                    a.score
                        .partial_cmp(&b.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .ok_or_else(|| {
                    AosError::Other("No retrieval available for expected question".to_string())
                })?;

            assert!(
                !retrievals.is_empty(),
                "No retrievals available for '{}' (expected snippet: '{}') in {}",
                expectation.question,
                expectation.answer_snippet,
                fixture.name
            );
            assert!(
                top.score > ANSWER_SCORE_THRESHOLD,
                "Expected high similarity for '{}', score={}",
                expectation.question,
                top.score
            );
        }

        for refusal in fixture.refusal_questions {
            let embedding = run.embedding_model.encode_text(refusal)?;
            let retrievals = tenant_index.retrieve(&embedding, 1)?;
            let top = retrievals.first().unwrap();
            assert!(
                top.score < REFUSAL_SCORE_THRESHOLD,
                "Expected refusal-level score for '{}', got {}",
                refusal,
                top.score
            );
        }
    }

    assert!(
        !run.training_data.examples.is_empty(),
        "Training data should be generated from fixtures"
    );

    Ok(())
}

#[tokio::test]
async fn fixture_pipeline_is_deterministic() -> Result<()> {
    let repo_root = repository_root()?;
    let fixture_paths = fixture_paths(&repo_root)?;
    let tokenizer = tokenizer_from_fixtures(&fixture_paths)?;

    let first = run_fixture_pipeline(&tokenizer, &fixture_paths).await?;
    let second = run_fixture_pipeline(&tokenizer, &fixture_paths).await?;

    assert_documents_equal(&first.documents, &second.documents);
    assert_rag_chunks_equal(&first.rag_chunks, &second.rag_chunks);
    assert_training_data_equal(&first.training_data, &second.training_data);

    Ok(())
}

async fn run_fixture_pipeline(
    tokenizer: &Arc<Tokenizer>,
    fixture_paths: &[PathBuf],
) -> Result<FixtureRun> {
    let chunk_options = ChunkingOptions {
        chunk_tokens: 128,
        overlap_tokens: 32,
        min_chunk_chars: 40,
    };
    let ingestor = DocumentIngestor::new(chunk_options, Some(tokenizer.clone()));
    let mut documents = Vec::new();
    for path in fixture_paths {
        documents.push(ingest_document(&ingestor, path)?);
    }

    let embedding_model: Arc<dyn EmbeddingModel> =
        Arc::new(SimpleEmbeddingModel::new(tokenizer.clone()));

    let rag_chunks = prepare_documents_for_rag(
        "fixture-tenant",
        &documents,
        &embedding_model,
        Some("fixture-rev"),
    )
    .await?;

    let training_config = TrainingGenConfig {
        strategy: TrainingStrategy::QuestionAnswer,
        max_seq_length: 512,
        add_special_tokens: true,
    };

    let training_data =
        generate_training_data_from_documents(&documents, tokenizer, &training_config)?;

    Ok(FixtureRun {
        documents,
        rag_chunks,
        training_data,
        embedding_model,
    })
}

fn ingest_document(
    ingestor: &DocumentIngestor,
    path: &Path,
) -> Result<adapteros_ingest_docs::IngestedDocument> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
    {
        Some(ext) if ext == "pdf" => ingestor.ingest_pdf_path(path),
        Some(ext) if ext == "md" || ext == "markdown" => ingestor.ingest_markdown_path(path),
        _ => Err(AosError::Validation(format!(
            "Unsupported fixture extension for {}",
            path.display()
        ))),
    }
}

fn fixture_paths(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let fixtures_root = repo_root.join("tests/fixtures");
    let mut paths = Vec::new();
    for fixture in DOCUMENT_FIXTURES {
        paths.push(fixtures_root.join(fixture.relative_path));
    }
    Ok(paths)
}

fn repository_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .ok_or_else(|| AosError::Other("Failed to determine repository root".to_string()))
}

fn assert_documents_equal(
    first: &[adapteros_ingest_docs::IngestedDocument],
    second: &[adapteros_ingest_docs::IngestedDocument],
) {
    assert_eq!(first.len(), second.len(), "Document count changed");
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.source_name, b.source_name);
        assert_eq!(a.doc_hash, b.doc_hash);
        assert_eq!(a.chunks.len(), b.chunks.len());
        for (chunk_a, chunk_b) in a.chunks.iter().zip(b.chunks.iter()) {
            assert_eq!(chunk_a.text, chunk_b.text);
            assert_eq!(chunk_a.start_offset, chunk_b.start_offset);
            assert_eq!(chunk_a.end_offset, chunk_b.end_offset);
        }
    }
}

fn assert_rag_chunks_equal(first: &[RagChunkParams], second: &[RagChunkParams]) {
    assert_eq!(first.len(), second.len());
    let mut left = first.to_owned();
    let mut right = second.to_owned();
    left.sort_by(|a, b| {
        a.doc_id
            .cmp(&b.doc_id)
            .then(a.chunk_index.cmp(&b.chunk_index))
    });
    right.sort_by(|a, b| {
        a.doc_id
            .cmp(&b.doc_id)
            .then(a.chunk_index.cmp(&b.chunk_index))
    });

    for (a, b) in left.iter().zip(right.iter()) {
        assert_eq!(a.doc_id, b.doc_id);
        assert_eq!(a.text, b.text);
        assert_eq!(a.rev, b.rev);
        assert_eq!(a.effectivity, b.effectivity);
        assert_eq!(a.source_type, b.source_type);
        assert_eq!(a.embedding, b.embedding);
    }
}

fn assert_training_data_equal(first: &TrainingData, second: &TrainingData) {
    assert_eq!(first.examples.len(), second.examples.len());
    for (a, b) in first.examples.iter().zip(second.examples.iter()) {
        assert_eq!(a.input, b.input);
        assert_eq!(a.target, b.target);
        assert_eq!(a.metadata, b.metadata);
    }
}
