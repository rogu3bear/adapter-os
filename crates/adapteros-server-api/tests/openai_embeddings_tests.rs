//! OpenAI-compatible Embeddings endpoint tests
//!
//! Tests for POST /v1/embeddings
//! Validates OpenAI-compatible request/response format.
//!
//! [2026-01-29 openai_embeddings]

use adapteros_server_api::handlers::openai_compat::{
    EmbeddingData, OpenAiCompletionPrompt, OpenAiEmbeddingItem, OpenAiEmbeddingUsage,
    OpenAiEmbeddingsRequest, OpenAiEmbeddingsResponse,
};

/// Test that single string input is accepted
#[test]
fn test_single_string_input() {
    let json_str = r#"{"input": "Hello, world!", "model": "text-embedding-ada-002"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.model, Some("text-embedding-ada-002".to_string()));
    match request.input {
        OpenAiCompletionPrompt::Single(text) => {
            assert_eq!(text, "Hello, world!");
        }
        _ => panic!("Expected single string input"),
    }
}

/// Test that array input is accepted
#[test]
fn test_array_input() {
    let json_str = r#"{"input": ["Hello", "World"], "model": "text-embedding-ada-002"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    match request.input {
        OpenAiCompletionPrompt::Batch(texts) => {
            assert_eq!(texts.len(), 2);
            assert_eq!(texts[0], "Hello");
            assert_eq!(texts[1], "World");
        }
        _ => panic!("Expected array input"),
    }
}

/// Test that encoding_format=float is accepted (default)
#[test]
fn test_encoding_format_float() {
    let json_str =
        r#"{"input": "test", "model": "text-embedding-ada-002", "encoding_format": "float"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.encoding_format, Some("float".to_string()));
}

/// Test that encoding_format=base64 is accepted
#[test]
fn test_encoding_format_base64() {
    let json_str =
        r#"{"input": "test", "model": "text-embedding-ada-002", "encoding_format": "base64"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.encoding_format, Some("base64".to_string()));
}

/// Test that user field is accepted
#[test]
fn test_user_field() {
    let json_str = r#"{"input": "test", "user": "user-123"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.user, Some("user-123".to_string()));
}

/// Test that dimensions field is accepted
#[test]
fn test_dimensions_field() {
    let json_str = r#"{"input": "test", "dimensions": 256}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(request.dimensions, Some(256));
}

/// Test minimal request (only input required)
#[test]
fn test_minimal_request() {
    let json_str = r#"{"input": "test"}"#;
    let request: OpenAiEmbeddingsRequest = serde_json::from_str(json_str).unwrap();

    assert!(request.model.is_none());
    assert!(request.encoding_format.is_none());
    assert!(request.user.is_none());
    assert!(request.dimensions.is_none());
}

/// Test float embedding data serialization
#[test]
fn test_embedding_data_float_serialization() {
    let embedding = EmbeddingData::from_float(vec![0.1, 0.2, 0.3]);
    let item = OpenAiEmbeddingItem {
        object: "embedding".to_string(),
        index: 0,
        embedding,
    };

    let json_str = serde_json::to_string(&item).unwrap();

    // Should serialize as array of floats
    assert!(json_str.contains("\"embedding\":["));
    assert!(json_str.contains("0.1"));
    assert!(json_str.contains("0.2"));
    assert!(json_str.contains("0.3"));
}

/// Test base64 embedding data serialization
#[test]
fn test_embedding_data_base64_serialization() {
    let embedding = EmbeddingData::from_base64(vec![0.1, 0.2, 0.3]);
    let item = OpenAiEmbeddingItem {
        object: "embedding".to_string(),
        index: 0,
        embedding,
    };

    let json_str = serde_json::to_string(&item).unwrap();

    // Should serialize as base64 string
    assert!(json_str.contains("\"embedding\":\""));
    // Base64 string should not contain array brackets
    assert!(!json_str.contains("[0."));
}

/// Test response format matches OpenAI spec
#[test]
fn test_response_format() {
    let response = OpenAiEmbeddingsResponse {
        object: "list".to_string(),
        data: vec![OpenAiEmbeddingItem {
            object: "embedding".to_string(),
            index: 0,
            embedding: EmbeddingData::from_float(vec![0.1, 0.2, 0.3]),
        }],
        model: "text-embedding-ada-002".to_string(),
        usage: OpenAiEmbeddingUsage {
            prompt_tokens: 5,
            total_tokens: 5,
        },
    };

    let json_str = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    // Verify required OpenAI fields
    assert_eq!(parsed["object"], "list");
    assert!(parsed["data"].is_array());
    assert_eq!(parsed["data"][0]["object"], "embedding");
    assert_eq!(parsed["data"][0]["index"], 0);
    assert!(parsed["data"][0]["embedding"].is_array());
    assert_eq!(parsed["model"], "text-embedding-ada-002");
    assert_eq!(parsed["usage"]["prompt_tokens"], 5);
    assert_eq!(parsed["usage"]["total_tokens"], 5);
}

/// Test multiple embeddings in response
#[test]
fn test_multiple_embeddings_response() {
    let response = OpenAiEmbeddingsResponse {
        object: "list".to_string(),
        data: vec![
            OpenAiEmbeddingItem {
                object: "embedding".to_string(),
                index: 0,
                embedding: EmbeddingData::from_float(vec![0.1, 0.2]),
            },
            OpenAiEmbeddingItem {
                object: "embedding".to_string(),
                index: 1,
                embedding: EmbeddingData::from_float(vec![0.3, 0.4]),
            },
        ],
        model: "adapteros-embed".to_string(),
        usage: OpenAiEmbeddingUsage {
            prompt_tokens: 10,
            total_tokens: 10,
        },
    };

    let json_str = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["data"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["data"][0]["index"], 0);
    assert_eq!(parsed["data"][1]["index"], 1);
}

/// Test base64 encoding produces correct format for float32 little-endian
#[test]
fn test_base64_encoding_correctness() {
    // A known float value for verification
    let floats = vec![1.0f32, 2.0f32];
    let embedding = EmbeddingData::from_base64(floats.clone());

    // Verify we can decode it back
    if let EmbeddingData::Base64 { embedding: b64_str } = embedding {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64_str)
            .unwrap();

        // Each f32 is 4 bytes, so we should have 8 bytes total
        assert_eq!(decoded.len(), 8);

        // Decode back to f32s (little-endian)
        let decoded_floats: Vec<f32> = decoded
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        assert_eq!(decoded_floats, floats);
    } else {
        panic!("Expected base64 embedding data");
    }
}
