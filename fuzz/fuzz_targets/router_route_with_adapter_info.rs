#![no_main]

use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights, MAX_K, ROUTER_GATE_Q15_DENOM,
};
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

const MAX_FEATURES: usize = 32;

fn next_clamped(u: &mut Unstructured<'_>, min: f32, max: f32, fallback: f32) -> Option<f32> {
    let raw: f32 = u.arbitrary().ok()?;
    if !raw.is_finite() {
        return Some(fallback);
    }
    Some(raw.clamp(min, max))
}

fn build_adapter(u: &mut Unstructured<'_>, idx: usize) -> Option<AdapterInfo> {
    let id_suffix: u16 = u.arbitrary().ok()?;
    let framework_selector: u8 = u.arbitrary().ok()?;
    let framework = match framework_selector % 3 {
        0 => Some("coreml".to_string()),
        1 => Some("mlx".to_string()),
        _ => None,
    };

    let lang_len = u.int_in_range::<usize>(0..=4).ok()?;
    let mut languages = Vec::with_capacity(lang_len);
    for _ in 0..lang_len {
        let lang_idx = u.int_in_range::<u8>(0..=7).ok()? as usize;
        languages.push(lang_idx);
    }
    languages.sort();
    languages.dedup();

    let tier_choice: u8 = u.arbitrary().ok()?;
    let tier = match tier_choice % 3 {
        0 => "prod",
        1 => "exp",
        _ => "dev",
    }
    .to_string();

    let scope_path = if u.arbitrary::<bool>().ok()? {
        Some(format!("scope/{}", idx))
    } else {
        None
    };

    let lora_tier = if u.arbitrary::<bool>().ok()? {
        Some("priority-a".to_string())
    } else {
        None
    };

    Some(AdapterInfo {
        id: format!("adapter-{}-{}", idx, id_suffix),
        stable_id: idx as u64,
        framework,
        languages,
        tier,
        scope_path,
        lora_tier,
        base_model: None,
        recommended_for_moe: false,
        reasoning_specialties: vec![],
        adapter_type: None,
        stream_session_id: None,
        base_adapter_id: None,
    })
}

fn build_weights(u: &mut Unstructured<'_>) -> Option<RouterWeights> {
    let weight = |u: &mut Unstructured<'_>| next_clamped(u, -2.0, 2.0, 0.0);
    Some(RouterWeights::new_with_dir_weights(
        weight(u)?,
        weight(u)?,
        weight(u)?,
        weight(u)?,
        weight(u)?,
        weight(u)?,
        weight(u)?,
        weight(u)?,
    ))
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    let adapter_count = match u.int_in_range::<usize>(1..=MAX_K) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut adapters = Vec::with_capacity(adapter_count);
    for idx in 0..adapter_count {
        if let Some(adapter) = build_adapter(&mut u, idx) {
            adapters.push(adapter);
        } else {
            return;
        }
    }

    let priors: Vec<f32> = (0..adapter_count)
        .filter_map(|_| next_clamped(&mut u, -8.0, 8.0, 0.0))
        .collect();
    if priors.len() != adapter_count {
        return;
    }

    let feature_len = match u.int_in_range::<usize>(22..=MAX_FEATURES) {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut features: Vec<f32> = (0..feature_len)
        .filter_map(|_| next_clamped(&mut u, -10.0, 10.0, 0.0))
        .collect();
    if features.len() < 25 {
        features.resize(25, 0.0);
    } else {
        features.truncate(25);
    }

    let k_raw = match u.int_in_range::<usize>(1..=MAX_K) {
        Ok(v) => v,
        Err(_) => return,
    };
    let k = k_raw.min(adapter_count.max(1)).max(1);

    let tau = match next_clamped(&mut u, 0.001, 8.0, 1.0) {
        Some(v) => v,
        None => return,
    };
    let eps = match next_clamped(&mut u, 0.0001, 0.5, 0.05) {
        Some(v) => v,
        None => return,
    };

    let weights = match build_weights(&mut u) {
        Some(w) => w,
        None => return,
    };

    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    let mask_mode = u.arbitrary::<u8>().unwrap_or(0) % 3;
    let policy_mask = match mask_mode {
        0 => PolicyMask::allow_all(&ids, None),
        1 => PolicyMask::deny_all(&ids, None),
        _ => {
            let mut allow_ids = Vec::new();
            let mut deny_ids = Vec::new();

            for id in &ids {
                if u.arbitrary::<bool>().ok().unwrap_or(false) {
                    allow_ids.push(id.clone());
                }
                if u.arbitrary::<bool>().ok().unwrap_or(false) {
                    deny_ids.push(id.clone());
                }
            }

            PolicyMask::build(
                &ids,
                if allow_ids.is_empty() {
                    None
                } else {
                    Some(&allow_ids)
                },
                if deny_ids.is_empty() {
                    None
                } else {
                    Some(&deny_ids)
                },
                None,
                None,
                None,
            )
        }
    };

    let mut router_a = Router::new_with_weights(weights.clone(), k, tau, eps);
    let mut router_b = Router::new_with_weights(weights, k, tau, eps);

    let decision_a =
        match router_a.route_with_adapter_info(&features, &priors, &adapters, &policy_mask) {
            Ok(d) => d,
            Err(_) => return, // Skip on routing errors
        };
    let decision_b =
        match router_b.route_with_adapter_info(&features, &priors, &adapters, &policy_mask) {
            Ok(d) => d,
            Err(_) => return,
        };

    assert_eq!(decision_a.indices.len(), decision_a.gates_q15.len());
    assert_eq!(decision_a.indices, decision_b.indices);
    assert_eq!(decision_a.gates_q15, decision_b.gates_q15);
    assert_eq!(
        decision_a.policy_mask_digest_b3,
        decision_b.policy_mask_digest_b3
    );
    assert_eq!(decision_a.entropy.to_bits(), decision_b.entropy.to_bits());

    match (
        decision_a.decision_hash.as_ref(),
        decision_b.decision_hash.as_ref(),
    ) {
        (Some(a), Some(b)) => {
            assert_eq!(a.input_hash, b.input_hash);
            assert_eq!(a.output_hash, b.output_hash);
            assert_eq!(a.combined_hash, b.combined_hash);
            assert_eq!(a.tau.to_bits(), b.tau.to_bits());
            assert_eq!(a.eps.to_bits(), b.eps.to_bits());
            assert_eq!(a.k, b.k);
        }
        (None, None) => {}
        _ => panic!("decision hash presence mismatch"),
    }

    for gate in &decision_a.gates_q15 {
        assert!(*gate <= ROUTER_GATE_Q15_DENOM as i16);
        assert!(*gate >= -(ROUTER_GATE_Q15_DENOM as i16));
    }

    for gate in decision_a.gates_f32() {
        assert!(gate.is_finite());
    }
});
