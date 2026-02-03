-- Readable ID migration: add legacy_id columns and alias table

CREATE TABLE IF NOT EXISTS id_aliases (
    kind TEXT NOT NULL,
    legacy_id TEXT UNIQUE NOT NULL,
    new_id TEXT UNIQUE NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE users ADD COLUMN legacy_id TEXT;
ALTER TABLE tenants ADD COLUMN legacy_id TEXT;
ALTER TABLE nodes ADD COLUMN legacy_id TEXT;
ALTER TABLE models ADD COLUMN legacy_id TEXT;
ALTER TABLE adapters ADD COLUMN legacy_id TEXT;
ALTER TABLE manifests ADD COLUMN legacy_id TEXT;
ALTER TABLE plans ADD COLUMN legacy_id TEXT;
ALTER TABLE cp_pointers ADD COLUMN legacy_id TEXT;
ALTER TABLE policies ADD COLUMN legacy_id TEXT;
ALTER TABLE jobs ADD COLUMN legacy_id TEXT;
ALTER TABLE telemetry_bundles ADD COLUMN legacy_id TEXT;
ALTER TABLE audits ADD COLUMN legacy_id TEXT;
ALTER TABLE workers ADD COLUMN legacy_id TEXT;
ALTER TABLE incidents ADD COLUMN legacy_id TEXT;
ALTER TABLE training_datasets ADD COLUMN legacy_id TEXT;
ALTER TABLE dataset_files ADD COLUMN legacy_id TEXT;
ALTER TABLE documents ADD COLUMN legacy_id TEXT;
ALTER TABLE document_chunks ADD COLUMN legacy_id TEXT;
ALTER TABLE document_collections ADD COLUMN legacy_id TEXT;
ALTER TABLE chat_sessions ADD COLUMN legacy_id TEXT;
ALTER TABLE chat_messages ADD COLUMN legacy_id TEXT;
ALTER TABLE adapter_stacks ADD COLUMN legacy_id TEXT;
ALTER TABLE routing_decisions ADD COLUMN legacy_id TEXT;
ALTER TABLE inference_traces ADD COLUMN legacy_id TEXT;
ALTER TABLE inference_trace_tokens ADD COLUMN legacy_id TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN legacy_id TEXT;
ALTER TABLE replay_executions ADD COLUMN legacy_id TEXT;
