-- Add tokenizer metadata to workers for heartbeat reporting

ALTER TABLE workers
    ADD COLUMN tokenizer_hash_b3 TEXT;

ALTER TABLE workers
    ADD COLUMN tokenizer_vocab_size INTEGER;

-- Optional index for quick lookup by tokenizer hash (routing/diagnostics)
CREATE INDEX IF NOT EXISTS idx_workers_tokenizer_hash
    ON workers(tokenizer_hash_b3);
