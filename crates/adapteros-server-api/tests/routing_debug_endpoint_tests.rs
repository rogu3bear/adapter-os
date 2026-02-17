use adapteros_server_api::types::response::{
    AdapterScore, FeatureScoreBreakdown, FeatureVector, RouterWeightsResponse,
    RoutingDebugResponseV2,
};

#[test]
fn routing_debug_response_includes_entropy_k_and_explainability_fields() {
    let response = RoutingDebugResponseV2 {
        features: FeatureVector {
            language: Some("rust".to_string()),
            frameworks: vec!["axum".to_string()],
            symbol_hits: 3,
            path_tokens: vec!["src".to_string(), "handlers".to_string()],
            verb: "refactor".to_string(),
        },
        adapter_scores: vec![AdapterScore {
            adapter_id: "adp_a".to_string(),
            score: 0.91,
            gate_value: 0.72,
            selected: true,
        }],
        selected_adapters: vec!["adp_a".to_string()],
        entropy: 1.234,
        k_value: 1,
        explanation: "explainability payload".to_string(),
        weights_used: Some(RouterWeightsResponse {
            tenant_id: "t-1".to_string(),
            language_weight: 0.3,
            framework_weight: 0.2,
            symbol_hits_weight: 0.15,
            path_tokens_weight: 0.1,
            prompt_verb_weight: 0.1,
            orthogonal_weight: 0.05,
            diversity_weight: 0.05,
            similarity_penalty: 0.05,
            total_weight: 1.0,
            is_default: false,
        }),
        feature_scores: vec![FeatureScoreBreakdown {
            adapter_id: "global_prompt".to_string(),
            language_score: 0.4,
            framework_score: 0.3,
            symbol_hits_score: 0.2,
            path_tokens_score: 0.1,
            prompt_verb_score: 0.05,
            tier_boost: 0.0,
            total_score: 1.05,
        }],
    };

    let value = serde_json::to_value(response).expect("serialize routing debug response");

    assert!(value.get("entropy").is_some(), "entropy must be present");
    assert!(value.get("k_value").is_some(), "k_value must be present");
    assert!(
        value.get("weights_used").is_some(),
        "weights_used must be present"
    );
    assert!(
        value.get("feature_scores").is_some(),
        "feature_scores must be present"
    );
    assert!(
        value["feature_scores"]
            .as_array()
            .is_some_and(|arr| !arr.is_empty()),
        "feature_scores must be non-empty"
    );
}

#[test]
fn routing_debug_feature_scores_are_prompt_global_shape() {
    let response = RoutingDebugResponseV2 {
        features: FeatureVector {
            language: Some("python".to_string()),
            frameworks: vec![],
            symbol_hits: 0,
            path_tokens: vec![],
            verb: "generate".to_string(),
        },
        adapter_scores: vec![
            AdapterScore {
                adapter_id: "adp_a".to_string(),
                score: 0.7,
                gate_value: 0.5,
                selected: true,
            },
            AdapterScore {
                adapter_id: "adp_b".to_string(),
                score: 0.6,
                gate_value: 0.0,
                selected: false,
            },
        ],
        selected_adapters: vec!["adp_a".to_string()],
        entropy: 1.1,
        k_value: 1,
        explanation: "global breakdown".to_string(),
        weights_used: None,
        feature_scores: vec![FeatureScoreBreakdown {
            adapter_id: "global_prompt".to_string(),
            language_score: 0.2,
            framework_score: 0.0,
            symbol_hits_score: 0.0,
            path_tokens_score: 0.1,
            prompt_verb_score: 0.2,
            tier_boost: 0.0,
            total_score: 0.5,
        }],
    };

    assert_eq!(response.feature_scores.len(), 1);
    assert_eq!(response.feature_scores[0].adapter_id, "global_prompt");
}
