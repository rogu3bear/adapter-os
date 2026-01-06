-- Extend adapter categories for domain/docs adapters and legacy bundles.

INSERT OR IGNORE INTO adapter_categories (name) VALUES
    ('domain'),
    ('domain-adapter'),
    ('docs'),
    ('documentation'),
    ('creative'),
    ('conversation'),
    ('analysis');
