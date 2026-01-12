-- Add validation_errors_json column to training_datasets
ALTER TABLE training_datasets ADD COLUMN validation_errors_json TEXT;
