-- Training datasets table for storing uploaded training data
CREATE TABLE IF NOT EXISTS training_datasets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    file_count INTEGER NOT NULL DEFAULT 0,
    total_size_bytes INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL,  -- 'patches', 'jsonl', 'txt', 'custom'
    hash_b3 TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    validation_status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'valid', 'invalid'
    validation_errors TEXT,
    metadata_json TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);

-- Dataset files table for tracking individual files in a dataset
CREATE TABLE IF NOT EXISTS dataset_files (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    hash_b3 TEXT NOT NULL,
    mime_type TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);

-- Dataset statistics for caching computed stats
CREATE TABLE IF NOT EXISTS dataset_statistics (
    dataset_id TEXT PRIMARY KEY,
    num_examples INTEGER NOT NULL DEFAULT 0,
    avg_input_length REAL NOT NULL DEFAULT 0.0,
    avg_target_length REAL NOT NULL DEFAULT 0.0,
    language_distribution TEXT,  -- JSON: {"python": 45, "rust": 30, ...}
    file_type_distribution TEXT,  -- JSON: {"py": 120, "rs": 80, ...}
    total_tokens INTEGER NOT NULL DEFAULT 0,
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);

-- Indices for performance
CREATE INDEX IF NOT EXISTS idx_training_datasets_created_at ON training_datasets(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_training_datasets_created_by ON training_datasets(created_by);
CREATE INDEX IF NOT EXISTS idx_training_datasets_format ON training_datasets(format);
CREATE INDEX IF NOT EXISTS idx_training_datasets_hash ON training_datasets(hash_b3);
CREATE INDEX IF NOT EXISTS idx_dataset_files_dataset_id ON dataset_files(dataset_id);
CREATE INDEX IF NOT EXISTS idx_dataset_statistics_dataset_id ON dataset_statistics(dataset_id);
