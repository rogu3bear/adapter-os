use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokenizers::Tokenizer;

#[derive(Deserialize)]
struct MyTrainingExample {
    id: Option<String>,
    input: InputWrapper,
    target: TargetWrapper,
    weight: f32,
    metadata: Option<HashMap<String, serde_json::Value>>,
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct InputWrapper {
    Text: String,
}

#[derive(Deserialize)]
struct TargetWrapper {
    Text: String,
}

#[derive(Serialize)]
struct TokenizedTrainingData {
    examples: Vec<TokenizedExample>,
}

#[derive(Serialize)]
struct TokenizedExample {
    input: Vec<u32>,
    target: Vec<u32>,
    metadata: Option<HashMap<String, serde_json::Value>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load tokenizer from AOS_TOKENIZER_PATH or default location
    let tokenizer_path = std::env::var("AOS_TOKENIZER_PATH")
        .unwrap_or_else(|_| "var/model-cache/models/qwen2.5-7b-instruct-bf16/tokenizer.json".to_string());
    let tokenizer = Tokenizer::from_file(&tokenizer_path)?;

    // Load my training data
    let data: Vec<MyTrainingExample> = serde_json::from_str(&std::fs::read_to_string("training/datasets/codebase/adapteros_docs/training_data.json")?)?;

    // Tokenize
    let mut tokenized_examples = Vec::new();

    for example in data {
        let input_tokens = tokenizer.encode(&example.input.Text)?;
        let target_tokens = tokenizer.encode(&example.target.Text)?;

        tokenized_examples.push(TokenizedExample {
            input: input_tokens,
            target: target_tokens,
            metadata: example.metadata,
        });
    }

    let tokenized_data = TokenizedTrainingData {
        examples: tokenized_examples,
    };

    // Save tokenized data
    std::fs::write(
        "training/datasets/codebase/adapteros_docs/training_data_tokenized.json",
        serde_json::to_string_pretty(&tokenized_data)?
    )?;

    println!("Tokenized {} examples", tokenized_data.examples.len());
    Ok(())
}
