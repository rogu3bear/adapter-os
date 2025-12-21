use adapteros_core::{
    context_manifest::{ContextAdapterEntryV1, ContextManifestV1},
    B3Hash, FusionInterval, SeedMode,
};
use proptest::prelude::*;

prop_compose! {
    fn adapter_entry_strategy()(id in "adapter-[a-z]{1,4}".prop_map(|s| s.to_string()),
                               rank in 0u32..32,
                               alpha_num in 1u64..10,
                               alpha_den in 1u64..10,
                               backend in prop_oneof![Just("coreml".to_string()), Just("mlx".to_string()), Just("metal".to_string())],
                               kernel in "k[0-9]{1,2}".prop_map(|s| s.to_string())) -> ContextAdapterEntryV1 {
        ContextAdapterEntryV1 {
            adapter_id: id,
            adapter_hash: B3Hash::hash(b"adapter-hash"),
            rank,
            alpha_num,
            alpha_den,
            backend_id: backend,
            kernel_version_id: kernel,
        }
    }
}

prop_compose! {
    fn manifest_strategy()(base_model in "qwen[0-9\\.a-zA-Z\\-]{0,6}".prop_map(|s| format!("model-{s}")),
                           adapter_dir_hash in any::<[u8;32]>(),
                           stack in prop::collection::vec(adapter_entry_strategy(), 0..4),
                           policy_digest in any::<[u8;32]>(),
                           sampler_params_digest in any::<[u8;32]>(),
                           build_id in "build-[0-9]{1,6}".prop_map(|s| s.to_string()),
                           build_git_sha in "gitsha-[0-9a-f]{6,12}".prop_map(|s| s.to_string()),
                           fusion_interval in prop_oneof![
                               Just(FusionInterval::PerRequest),
                               Just(FusionInterval::PerToken),
                               (1u32..64).prop_map(|t| FusionInterval::PerSegment { tokens_per_segment: t }),
                           ]) -> ContextManifestV1 {
        ContextManifestV1 {
            base_model_id: base_model,
            base_model_hash: B3Hash::hash(b"base-model"),
            adapter_dir_hash: B3Hash::from_bytes(adapter_dir_hash),
            adapter_stack: stack,
            router_version: "router-1.0".to_string(),
            seed_mode: SeedMode::Strict,
            seed_inputs_digest: B3Hash::hash(b"seed-inputs"),
            policy_digest: B3Hash::from_bytes(policy_digest),
            sampler_params_digest: B3Hash::from_bytes(sampler_params_digest),
            build_id,
            build_git_sha,
            fusion_interval,
        }
    }
}

proptest! {
    #[test]
    fn canonical_bytes_stable(manifest in manifest_strategy()) {
        let bytes_a = manifest.to_bytes();
        let bytes_b = manifest.to_bytes();
        prop_assert_eq!(&bytes_a, &bytes_b);

        let digest = manifest.digest();
        prop_assert_eq!(digest, B3Hash::hash(&bytes_a));
    }

    #[test]
    fn digest_changes_when_manifest_changes(mut manifest in manifest_strategy()) {
        let baseline = manifest.digest();
        manifest.build_id.push_str("-alt");
        let changed = manifest.digest();
        prop_assert_ne!(baseline, changed);
    }
}
