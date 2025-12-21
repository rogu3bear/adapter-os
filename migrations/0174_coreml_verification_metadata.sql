-- Track CoreML verification details for replay observability
ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_expected_package_hash TEXT;

ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_hash_mismatch INTEGER;
