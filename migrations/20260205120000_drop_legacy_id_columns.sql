-- Drop legacy ID infrastructure: legacy_id columns, id_aliases, id_backfill_state.
-- All IDs now use TypedId format ({prefix}-{uuid_v7}).

DROP TABLE IF EXISTS id_aliases;
DROP TABLE IF EXISTS id_backfill_state;

ALTER TABLE users DROP COLUMN legacy_id;
ALTER TABLE tenants DROP COLUMN legacy_id;
ALTER TABLE nodes DROP COLUMN legacy_id;
ALTER TABLE models DROP COLUMN legacy_id;
ALTER TABLE adapters DROP COLUMN legacy_id;
ALTER TABLE manifests DROP COLUMN legacy_id;
ALTER TABLE plans DROP COLUMN legacy_id;
ALTER TABLE cp_pointers DROP COLUMN legacy_id;
ALTER TABLE policies DROP COLUMN legacy_id;
ALTER TABLE jobs DROP COLUMN legacy_id;
ALTER TABLE telemetry_bundles DROP COLUMN legacy_id;
ALTER TABLE audits DROP COLUMN legacy_id;
ALTER TABLE workers DROP COLUMN legacy_id;
ALTER TABLE incidents DROP COLUMN legacy_id;
ALTER TABLE training_datasets DROP COLUMN legacy_id;
ALTER TABLE dataset_files DROP COLUMN legacy_id;
ALTER TABLE documents DROP COLUMN legacy_id;
ALTER TABLE document_chunks DROP COLUMN legacy_id;
ALTER TABLE document_collections DROP COLUMN legacy_id;
ALTER TABLE chat_sessions DROP COLUMN legacy_id;
ALTER TABLE chat_messages DROP COLUMN legacy_id;
ALTER TABLE adapter_stacks DROP COLUMN legacy_id;
ALTER TABLE routing_decisions DROP COLUMN legacy_id;
ALTER TABLE inference_traces DROP COLUMN legacy_id;
ALTER TABLE inference_trace_tokens DROP COLUMN legacy_id;
ALTER TABLE inference_trace_receipts DROP COLUMN legacy_id;
ALTER TABLE replay_executions DROP COLUMN legacy_id;
