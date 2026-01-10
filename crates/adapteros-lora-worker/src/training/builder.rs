//! Dataset builder for deterministic ingestion and normalization.
//!
//! Transforms raw datasets from various sources into tokenized training examples
//! with provenance tracking and deterministic ordering.

use super::formats::{
    parse_file, ColumnMapping, DatasetFormat, ParserConfig, RawSample, TextStrategy,
};
use super::limits::DatasetSizeLimits;
use super::normalize::NORMALIZATION_SCHEME;
use crate::tokenizer::QwenTokenizer;
use adapteros_core::{AosError, Result};
use adapteros_secure_fs::path_policy::{canonicalize_strict, canonicalize_strict_in_allowed_roots};
use adapteros_secure_fs::traversal::check_path_traversal;
use adapteros_types::training::{
    provenance_from_map, validate_training_examples, ExampleMetadataV1, TrainingDataContractConfig,
    TrainingExampleV1, TRAINING_DATA_CONTRACT_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};
use walkdir::WalkDir;

/// Dataset source specification.
#[derive(Debug, Clone)]
pub enum DatasetSource {
    /// Local filesystem path (file or directory).
    Filesystem(PathBuf),
    /// Git repository.
    Git {
        url: String,
        branch: Option<String>,
        path: Option<String>,
        auth: GitAuth,
    },
    /// Archive file (.zip, .tar.gz).
    Archive(PathBuf),
}

/// Git authentication method.
#[derive(Debug, Clone, Default)]
pub enum GitAuth {
    /// No authentication (public repos).
    #[default]
    None,
    /// SSH key authentication.
    SshKey(Option<PathBuf>),
    /// HTTPS token authentication.
    HttpsToken(String),
    /// System credential helper.
    CredentialHelper,
}

/// Build configuration for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Source format used.
    pub format: String,
    /// Normalization scheme applied.
    pub normalization: String,
    /// Ordering method.
    pub ordering: String,
    /// Column mapping (for CSV).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_mapping: Option<ColumnMapping>,
    /// Text strategy (for text/markdown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_strategy: Option<String>,
}

/// Extended dataset manifest with tokenizer tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltDatasetManifest {
    /// Dataset name.
    pub name: String,
    /// Manifest version.
    pub version: String,
    /// Training data contract version.
    pub training_contract_version: String,
    /// BLAKE3 hash of the tokenizer.json file.
    pub tokenizer_hash_b3: String,
    /// Build configuration for reproducibility.
    pub build_config: BuildConfig,
    /// Number of samples.
    pub sample_count: usize,
    /// Source files processed.
    pub source_files: Vec<SourceFileInfo>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}

/// Information about a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFileInfo {
    /// Relative path from source root.
    pub path: String,
    /// BLAKE3 hash of file content.
    pub hash_b3: String,
    /// Number of samples extracted.
    pub sample_count: usize,
}

/// Result of a dataset build operation.
#[derive(Debug)]
pub struct BuildResult {
    /// Path to the generated manifest.
    pub manifest_path: PathBuf,
    /// Path to the examples file.
    pub examples_path: PathBuf,
    /// Number of examples generated.
    pub example_count: usize,
    /// BLAKE3 hash of tokenizer.
    pub tokenizer_hash: String,
}

/// Dataset builder for deterministic ingestion.
pub struct DatasetBuilder {
    tokenizer_path: PathBuf,
    output_dir: PathBuf,
    format: Option<DatasetFormat>,
    column_mapping: Option<ColumnMapping>,
    text_strategy: TextStrategy,
    name: Option<String>,
    limits: DatasetSizeLimits,
}

impl DatasetBuilder {
    /// Create a new dataset builder.
    pub fn new(tokenizer_path: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            tokenizer_path,
            output_dir,
            format: None,
            column_mapping: None,
            text_strategy: TextStrategy::default(),
            name: None,
            limits: DatasetSizeLimits::from_env(),
        }
    }

    /// Set explicit format (disables auto-detection).
    pub fn with_format(mut self, format: DatasetFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Set column mapping for CSV format.
    pub fn with_column_mapping(mut self, mapping: ColumnMapping) -> Self {
        self.column_mapping = Some(mapping);
        self
    }

    /// Set text parsing strategy.
    pub fn with_text_strategy(mut self, strategy: TextStrategy) -> Self {
        self.text_strategy = strategy;
        self
    }

    /// Set dataset name.
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Override dataset size limits (useful for tests).
    pub fn with_limits(mut self, limits: DatasetSizeLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Build dataset from source (validates without writing).
    pub fn validate(&self, source: &DatasetSource) -> Result<usize> {
        let (samples, _) = self.collect_samples(source)?;
        Ok(samples.len())
    }

    /// Build dataset from source.
    pub fn build(&self, source: &DatasetSource) -> Result<BuildResult> {
        info!("Building dataset from {:?}", source);

        // Ensure output directory exists
        fs::create_dir_all(&self.output_dir).map_err(|e| {
            AosError::Io(format!(
                "Failed to create output directory {}: {}",
                self.output_dir.display(),
                e
            ))
        })?;
        let output_dir = canonicalize_strict(&self.output_dir)?;

        // Load tokenizer
        let tokenizer_path = canonicalize_strict(&self.tokenizer_path)?;
        let tokenizer = QwenTokenizer::from_file(&tokenizer_path)?;
        let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
            AosError::Validation("Tokenizer missing pad_token_id for dataset build".to_string())
        })?;
        let vocab_size = tokenizer.vocab_size(true);

        // Compute tokenizer hash
        let tokenizer_hash = compute_file_hash(&tokenizer_path)?;

        // Collect and validate samples
        let (mut samples, source_files) = self.collect_samples(source)?;

        if samples.is_empty() {
            return Err(AosError::Validation(
                "No valid samples found in source".to_string(),
            ));
        }

        info!(
            "Collected {} raw samples from {} files",
            samples.len(),
            source_files.len()
        );

        // Apply deterministic ordering
        deterministic_sort(&mut samples);
        debug!(
            "Applied deterministic ordering to {} samples",
            samples.len()
        );

        // Tokenize samples
        let examples = tokenize_samples(&samples, &tokenizer, &self.limits, pad_token_id)?;
        let contract = TrainingDataContractConfig::new(pad_token_id, -1);
        validate_training_examples(&examples, vocab_size, &contract).map_err(|err| {
            AosError::Validation(format!("Dataset example validation failed: {}", err))
        })?;
        info!("Tokenized {} examples", examples.len());

        // Write examples
        let examples_path = output_dir.join("examples.jsonl");
        write_examples(&examples, &examples_path)?;

        // Generate manifest
        let manifest = self.create_manifest(&tokenizer_hash, &source_files, examples.len());
        let manifest_path = output_dir.join("DatasetManifest.json");
        write_manifest(&manifest, &manifest_path)?;

        // Write provenance
        let provenance_dir = output_dir.join("provenance");
        fs::create_dir_all(&provenance_dir)
            .map_err(|e| AosError::Io(format!("Failed to create provenance directory: {}", e)))?;
        let source_files_path = provenance_dir.join("source_files.json");
        let source_json = serde_json::to_string_pretty(&source_files).map_err(|e| {
            AosError::Validation(format!("Failed to serialize source files: {}", e))
        })?;
        fs::write(&source_files_path, source_json)
            .map_err(|e| AosError::Io(format!("Failed to write source files: {}", e)))?;

        info!(
            "Dataset built: {} examples, manifest at {}",
            examples.len(),
            manifest_path.display()
        );

        Ok(BuildResult {
            manifest_path,
            examples_path,
            example_count: examples.len(),
            tokenizer_hash,
        })
    }

    /// Collect samples from source.
    fn collect_samples(
        &self,
        source: &DatasetSource,
    ) -> Result<(Vec<RawSample>, Vec<SourceFileInfo>)> {
        match source {
            DatasetSource::Filesystem(path) => self.collect_from_filesystem(path),
            DatasetSource::Git {
                url,
                branch,
                path,
                auth,
            } => self.collect_from_git(url, branch.as_deref(), path.as_deref(), auth),
            DatasetSource::Archive(path) => self.collect_from_archive(path),
        }
    }

    /// Collect samples from filesystem path.
    fn collect_from_filesystem(
        &self,
        path: &Path,
    ) -> Result<(Vec<RawSample>, Vec<SourceFileInfo>)> {
        let canonical_path = canonicalize_strict(path)?;

        let files = if canonical_path.is_file() {
            vec![canonical_path.clone()]
        } else {
            collect_files_sorted(&canonical_path)?
        };

        validate_files_and_sizes(&files, &self.limits)?;
        self.parse_files(&files, &canonical_path)
    }

    /// Collect samples from git repository.
    fn collect_from_git(
        &self,
        url: &str,
        branch: Option<&str>,
        subpath: Option<&str>,
        auth: &GitAuth,
    ) -> Result<(Vec<RawSample>, Vec<SourceFileInfo>)> {
        // Create temp directory for clone
        let temp_dir = tempfile::tempdir()
            .map_err(|e| AosError::Io(format!("Failed to create temp directory: {}", e)))?;

        info!("Cloning {} to {}", url, temp_dir.path().display());

        // Build fetch options with authentication
        let mut fetch_options = git2::FetchOptions::new();
        let mut callbacks = git2::RemoteCallbacks::new();

        match auth {
            GitAuth::None => {}
            GitAuth::SshKey(key_path) => {
                let key = key_path.clone().unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_default()
                        .join(".ssh")
                        .join("id_rsa")
                });
                callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                    git2::Cred::ssh_key(username_from_url.unwrap_or("git"), None, &key, None)
                });
            }
            GitAuth::HttpsToken(token) => {
                let token = token.clone();
                callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                    git2::Cred::userpass_plaintext("git", &token)
                });
            }
            GitAuth::CredentialHelper => {
                callbacks.credentials(|url, username_from_url, allowed_types| {
                    git2::Cred::credential_helper(
                        &git2::Config::open_default()?,
                        url,
                        username_from_url,
                    )
                    .or_else(|_| {
                        if allowed_types.contains(git2::CredentialType::DEFAULT) {
                            git2::Cred::default()
                        } else {
                            Err(git2::Error::from_str("No credentials available"))
                        }
                    })
                });
            }
        }

        fetch_options.remote_callbacks(callbacks);

        // Clone repository
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_options);

        if let Some(b) = branch {
            builder.branch(b);
        }

        let _repo = builder
            .clone(url, temp_dir.path())
            .map_err(|e| AosError::Io(format!("Failed to clone repository {}: {}", url, e)))?;

        // Determine source path within repo
        let repo_root = canonicalize_strict(temp_dir.path())?;
        let allowed_roots = [repo_root.clone()];
        let source_path = if let Some(p) = subpath {
            repo_root.join(p)
        } else {
            repo_root.clone()
        };

        let source_path = canonicalize_strict_in_allowed_roots(&source_path, &allowed_roots)
            .map_err(|e| AosError::Validation(format!("Git source path rejected: {}", e)))?;

        self.collect_from_filesystem(&source_path)
    }

    /// Collect samples from archive.
    fn collect_from_archive(
        &self,
        archive_path: &Path,
    ) -> Result<(Vec<RawSample>, Vec<SourceFileInfo>)> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| AosError::Io(format!("Failed to create temp directory: {}", e)))?;

        let archive_path = canonicalize_strict(archive_path)?;
        let path_str = archive_path.display().to_string().to_lowercase();

        if path_str.ends_with(".zip") {
            extract_zip(&archive_path, temp_dir.path(), &self.limits)?;
        } else if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
            extract_tar_gz(&archive_path, temp_dir.path(), &self.limits)?;
        } else if path_str.ends_with(".tar") {
            extract_tar(&archive_path, temp_dir.path(), &self.limits)?;
        } else {
            return Err(AosError::Validation(format!(
                "Unsupported archive format: {}",
                archive_path.display()
            )));
        }

        self.collect_from_filesystem(temp_dir.path())
    }

    /// Parse collected files into samples.
    fn parse_files(
        &self,
        files: &[PathBuf],
        base_path: &Path,
    ) -> Result<(Vec<RawSample>, Vec<SourceFileInfo>)> {
        let config = ParserConfig {
            column_mapping: self.column_mapping.clone(),
            text_strategy: self.text_strategy,
        };

        let mut all_samples = Vec::new();
        let mut source_files = Vec::new();
        let mut total_samples: usize = 0;

        for file_path in files {
            let format = self.format.or_else(|| DatasetFormat::detect(file_path));

            let Some(format) = format else {
                debug!("Skipping file with unknown format: {}", file_path.display());
                continue;
            };

            let samples = parse_file(file_path, format, &config)?;
            total_samples = total_samples.saturating_add(samples.len());
            if total_samples > self.limits.max_samples {
                return Err(AosError::Validation(format!(
                    "Dataset sample count exceeds limit: {} > {}",
                    total_samples, self.limits.max_samples
                )));
            }

            let relative_path = file_path
                .strip_prefix(base_path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            let file_hash = compute_file_hash(file_path)?;

            source_files.push(SourceFileInfo {
                path: relative_path,
                hash_b3: file_hash,
                sample_count: samples.len(),
            });

            all_samples.extend(samples);
        }

        Ok((all_samples, source_files))
    }

    /// Create manifest from build results.
    fn create_manifest(
        &self,
        tokenizer_hash: &str,
        source_files: &[SourceFileInfo],
        sample_count: usize,
    ) -> BuiltDatasetManifest {
        let format_name = self
            .format
            .map(|f| f.name().to_string())
            .unwrap_or_else(|| "auto".to_string());

        let name = self
            .name
            .clone()
            .unwrap_or_else(|| format!("dataset_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")));

        BuiltDatasetManifest {
            name,
            version: "1.0".to_string(),
            training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
            tokenizer_hash_b3: tokenizer_hash.to_string(),
            build_config: BuildConfig {
                format: format_name,
                normalization: NORMALIZATION_SCHEME.to_string(),
                ordering: "input_hash_asc".to_string(),
                column_mapping: self.column_mapping.clone(),
                text_strategy: Some(self.text_strategy.to_string()),
            },
            sample_count,
            source_files: source_files.to_vec(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Collect files from directory in deterministic order.
fn collect_files_sorted(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            // Filter to supported formats
            let path = e.path();
            DatasetFormat::detect(path).is_some()
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort by filename for determinism
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    Ok(files)
}

fn validate_files_and_sizes(files: &[PathBuf], limits: &DatasetSizeLimits) -> Result<u64> {
    if files.len() > limits.max_files {
        return Err(AosError::Validation(format!(
            "Dataset file count exceeds limit: {} > {}",
            files.len(),
            limits.max_files
        )));
    }

    let mut total_bytes: u64 = 0;
    for file in files {
        let size = fs::metadata(file).map_err(|e| {
            AosError::Io(format!(
                "Failed to read file metadata {}: {}",
                file.display(),
                e
            ))
        })?;
        total_bytes = total_bytes.saturating_add(size.len());
        if total_bytes > limits.max_total_bytes {
            return Err(AosError::Validation(format!(
                "Dataset total size exceeds limit: {} > {} bytes",
                total_bytes, limits.max_total_bytes
            )));
        }
    }

    Ok(total_bytes)
}

/// Apply deterministic ordering to samples by input hash.
fn deterministic_sort(samples: &mut [RawSample]) {
    samples.sort_by(|a, b| {
        let hash_a = blake3::hash(a.input.as_bytes());
        let hash_b = blake3::hash(b.input.as_bytes());
        hash_a.as_bytes().cmp(hash_b.as_bytes())
    });
}

/// Tokenize raw samples into training examples.
fn tokenize_samples(
    samples: &[RawSample],
    tokenizer: &QwenTokenizer,
    limits: &DatasetSizeLimits,
    pad_token_id: u32,
) -> Result<Vec<TrainingExampleV1>> {
    let mut examples = Vec::with_capacity(samples.len());
    let mut total_tokens: u64 = 0;
    let created_at_unix_ms = chrono::Utc::now().timestamp_millis() as u64;

    for (i, sample) in samples.iter().enumerate() {
        let input_tokens = tokenizer.encode(&sample.input).map_err(|e| {
            AosError::Validation(format!("Failed to tokenize input at sample {}: {}", i, e))
        })?;

        let target_tokens = tokenizer.encode(&sample.target).map_err(|e| {
            AosError::Validation(format!("Failed to tokenize target at sample {}: {}", i, e))
        })?;

        total_tokens =
            total_tokens.saturating_add((input_tokens.len() + target_tokens.len()) as u64);
        if total_tokens > limits.max_tokens {
            return Err(AosError::Validation(format!(
                "Dataset token count exceeds limit: {} > {}",
                total_tokens, limits.max_tokens
            )));
        }

        let metadata = build_example_metadata(
            sample.metadata.clone(),
            sample.weight,
            i as u64,
            created_at_unix_ms,
        )?;
        let attention_mask =
            TrainingExampleV1::attention_mask_from_tokens(&input_tokens, pad_token_id);
        examples.push(TrainingExampleV1::new(
            input_tokens,
            target_tokens,
            attention_mask,
            metadata,
        ));
    }

    Ok(examples)
}

fn build_example_metadata(
    metadata: HashMap<String, String>,
    weight: f32,
    row_id: u64,
    created_at_unix_ms: u64,
) -> Result<ExampleMetadataV1> {
    let mut provenance = BTreeMap::new();
    for (key, value) in metadata.iter() {
        provenance.insert(key.clone(), serde_json::Value::String(value.clone()));
    }
    if let Some(num) = serde_json::Number::from_f64(weight as f64) {
        provenance.insert("weight".to_string(), serde_json::Value::Number(num));
    } else {
        provenance.insert(
            "weight".to_string(),
            serde_json::Value::String(weight.to_string()),
        );
    }

    let source_id = metadata
        .get("source_path")
        .or_else(|| metadata.get("dataset_name"))
        .or_else(|| metadata.get("source_file"))
        .cloned()
        .unwrap_or_else(|| "dataset_builder".to_string());

    let provenance_json = provenance_from_map(&provenance)
        .map_err(|e| AosError::Validation(format!("Failed to serialize provenance: {}", e)))?;

    Ok(ExampleMetadataV1::new(
        source_id,
        row_id,
        provenance_json,
        created_at_unix_ms,
    ))
}

/// Compute BLAKE3 hash of a file.
fn compute_file_hash(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open file for hashing {}: {}",
            path.display(),
            e
        ))
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|e| AosError::Io(format!("Failed to read file for hashing: {}", e)))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Write examples to JSONL file.
fn write_examples(examples: &[TrainingExampleV1], path: &Path) -> Result<()> {
    let mut file = File::create(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to create examples file {}: {}",
            path.display(),
            e
        ))
    })?;

    for example in examples {
        let line = serde_json::to_string(example)
            .map_err(|e| AosError::Validation(format!("Failed to serialize example: {}", e)))?;
        writeln!(file, "{}", line)
            .map_err(|e| AosError::Io(format!("Failed to write example: {}", e)))?;
    }

    Ok(())
}

/// Write manifest to JSON file.
fn write_manifest(manifest: &BuiltDatasetManifest, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| AosError::Validation(format!("Failed to serialize manifest: {}", e)))?;

    fs::write(path, json).map_err(|e| {
        AosError::Io(format!(
            "Failed to write manifest {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Extract zip archive.
fn extract_zip(archive_path: &Path, dest: &Path, limits: &DatasetSizeLimits) -> Result<()> {
    let file = File::open(archive_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open archive {}: {}",
            archive_path.display(),
            e
        ))
    })?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AosError::Validation(format!("Invalid zip archive: {}", e)))?;

    let canonical_dest = canonicalize_strict(dest)?;
    let allowed_roots = [canonical_dest.clone()];
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AosError::Io(format!("Failed to read zip entry {}: {}", i, e)))?;

        if is_zip_symlink(&entry) {
            return Err(AosError::Validation(format!(
                "Zip entry is a symlink and was rejected: {}",
                entry.name()
            )));
        }

        let entry_path = entry
            .enclosed_name()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| {
                let name = entry.name().to_string();
                error!(original = %name, canonical = "<unavailable>", "Zip entry path rejected");
                AosError::Validation(format!("Zip entry contains invalid path: {}", name))
            })?;
        validate_archive_entry_path(&entry_path, entry.name())?;

        let output_path = canonical_dest.join(&entry_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
            continue;
        }

        file_count += 1;
        if file_count > limits.max_files {
            return Err(AosError::Validation(format!(
                "Archive file count exceeds limit: {} > {}",
                file_count, limits.max_files
            )));
        }
        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > limits.max_total_bytes {
            return Err(AosError::Validation(format!(
                "Archive expands beyond size limit: {} > {} bytes",
                total_bytes, limits.max_total_bytes
            )));
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
                .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
        }

        if output_path.exists() {
            let metadata = fs::symlink_metadata(&output_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read metadata for {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            if metadata.file_type().is_symlink() {
                return Err(AosError::Validation(format!(
                    "Zip entry path is a symlink and was rejected: {}",
                    output_path.display()
                )));
            }
        }

        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|e| {
                AosError::Io(format!(
                    "Failed to create output file {}: {}",
                    output_path.display(),
                    e
                ))
            })?;

        std::io::copy(&mut entry, &mut output_file).map_err(|e| {
            AosError::Io(format!(
                "Failed to extract zip entry {}: {}",
                output_path.display(),
                e
            ))
        })?;

        canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
            .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
    }

    Ok(())
}

/// Extract tar.gz archive.
fn extract_tar_gz(archive_path: &Path, dest: &Path, limits: &DatasetSizeLimits) -> Result<()> {
    let file = File::open(archive_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open archive {}: {}",
            archive_path.display(),
            e
        ))
    })?;

    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    extract_tar_entries(&mut archive, dest, limits)
}

/// Extract tar archive.
fn extract_tar(archive_path: &Path, dest: &Path, limits: &DatasetSizeLimits) -> Result<()> {
    let file = File::open(archive_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to open archive {}: {}",
            archive_path.display(),
            e
        ))
    })?;

    let mut archive = tar::Archive::new(file);
    extract_tar_entries(&mut archive, dest, limits)
}

fn extract_tar_entries<R: Read>(
    archive: &mut tar::Archive<R>,
    dest: &Path,
    limits: &DatasetSizeLimits,
) -> Result<()> {
    let canonical_dest = canonicalize_strict(dest)?;
    let allowed_roots = [canonical_dest.clone()];
    let mut total_bytes: u64 = 0;
    let mut file_count: usize = 0;

    for entry in archive
        .entries()
        .map_err(|e| AosError::Validation(format!("Failed to read tar entries: {}", e)))?
    {
        let mut entry =
            entry.map_err(|e| AosError::Validation(format!("Failed to read tar entry: {}", e)))?;
        let entry_path = entry
            .path()
            .map_err(|e| AosError::Validation(format!("Failed to read tar entry path: {}", e)))?;
        validate_archive_entry_path(&entry_path, &entry_path.to_string_lossy())?;

        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(AosError::Validation(format!(
                "Tar entry is a link and was rejected: {}",
                entry_path.display()
            )));
        }

        let output_path = canonical_dest.join(&entry_path);
        if entry_type.is_dir() {
            fs::create_dir_all(&output_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
                .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
            continue;
        }

        if !entry_type.is_file() {
            return Err(AosError::Validation(format!(
                "Unsupported tar entry type for {}",
                entry_path.display()
            )));
        }

        file_count += 1;
        if file_count > limits.max_files {
            return Err(AosError::Validation(format!(
                "Archive file count exceeds limit: {} > {}",
                file_count, limits.max_files
            )));
        }
        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > limits.max_total_bytes {
            return Err(AosError::Validation(format!(
                "Archive expands beyond size limit: {} > {} bytes",
                total_bytes, limits.max_total_bytes
            )));
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
            canonicalize_strict_in_allowed_roots(parent, &allowed_roots)
                .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
        }

        if output_path.exists() {
            let metadata = fs::symlink_metadata(&output_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to read metadata for {}: {}",
                    output_path.display(),
                    e
                ))
            })?;
            if metadata.file_type().is_symlink() {
                return Err(AosError::Validation(format!(
                    "Tar entry path is a symlink and was rejected: {}",
                    output_path.display()
                )));
            }
        }

        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|e| {
                AosError::Io(format!(
                    "Failed to create output file {}: {}",
                    output_path.display(),
                    e
                ))
            })?;

        std::io::copy(&mut entry, &mut output_file).map_err(|e| {
            AosError::Io(format!(
                "Failed to extract tar entry {}: {}",
                output_path.display(),
                e
            ))
        })?;

        canonicalize_strict_in_allowed_roots(&output_path, &allowed_roots)
            .map_err(|e| AosError::Validation(format!("Archive path rejected: {}", e)))?;
    }

    Ok(())
}

fn validate_archive_entry_path(entry_path: &Path, entry_name: &str) -> Result<()> {
    if entry_path.is_absolute() {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            "Archive entry path rejected (absolute)"
        );
        return Err(AosError::Validation(format!(
            "Archive entry path is absolute: {}",
            entry_name
        )));
    }

    check_path_traversal(entry_path).map_err(|e| {
        error!(
            original = %entry_name,
            canonical = "<unavailable>",
            error = %e,
            "Archive entry path rejected (traversal)"
        );
        AosError::Validation(format!("Archive entry path rejected: {}", entry_name))
    })?;

    Ok(())
}

fn is_zip_symlink(entry: &zip::read::ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .map(|mode| (mode & 0o170000) == 0o120000)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::tempdir;

    fn create_test_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let path = dir.join(name);
        let mut file = File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }
        path
    }

    #[test]
    fn test_deterministic_sort() {
        let mut samples = vec![
            RawSample::new("zebra".to_string(), "a".to_string()),
            RawSample::new("apple".to_string(), "b".to_string()),
            RawSample::new("mango".to_string(), "c".to_string()),
        ];

        deterministic_sort(&mut samples);

        // Verify ordering is consistent
        let mut samples2 = vec![
            RawSample::new("mango".to_string(), "c".to_string()),
            RawSample::new("zebra".to_string(), "a".to_string()),
            RawSample::new("apple".to_string(), "b".to_string()),
        ];

        deterministic_sort(&mut samples2);

        // Same ordering regardless of initial order
        for (a, b) in samples.iter().zip(samples2.iter()) {
            assert_eq!(a.input, b.input);
        }
    }

    #[test]
    fn test_collect_files_sorted() {
        let dir = tempdir().unwrap();

        // Create files in non-alphabetical order
        create_test_jsonl(dir.path(), "c.jsonl", &[r#"{"input":"a","output":"b"}"#]);
        create_test_jsonl(dir.path(), "a.jsonl", &[r#"{"input":"a","output":"b"}"#]);
        create_test_jsonl(dir.path(), "b.jsonl", &[r#"{"input":"a","output":"b"}"#]);

        let files = collect_files_sorted(dir.path()).unwrap();

        assert_eq!(files.len(), 3);
        assert!(files[0].ends_with("a.jsonl"));
        assert!(files[1].ends_with("b.jsonl"));
        assert!(files[2].ends_with("c.jsonl"));
    }

    #[test]
    fn test_validate_rejects_sample_limit() {
        let dir = tempdir().unwrap();
        let data_path = create_test_jsonl(
            dir.path(),
            "data.jsonl",
            &[
                r#"{"input":"one","output":"a"}"#,
                r#"{"input":"two","output":"b"}"#,
            ],
        );

        let limits = DatasetSizeLimits {
            max_files: 10,
            max_total_bytes: 1024 * 1024,
            max_samples: 1,
            max_tokens: 1000,
        };

        let builder = DatasetBuilder::new(PathBuf::from("tokenizer.json"), dir.path().join("out"))
            .with_limits(limits);
        let err = builder
            .validate(&DatasetSource::Filesystem(data_path))
            .unwrap_err();
        assert!(err.to_string().contains("sample count exceeds limit"));
    }

    #[test]
    fn test_compute_file_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let hash1 = compute_file_hash(&path).unwrap();
        let hash2 = compute_file_hash(&path).unwrap();

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // BLAKE3 hex length
    }

    #[test]
    fn test_build_config_serialization() {
        let config = BuildConfig {
            format: "jsonl".to_string(),
            normalization: NORMALIZATION_SCHEME.to_string(),
            ordering: "input_hash_asc".to_string(),
            column_mapping: None,
            text_strategy: Some("paragraph-pairs".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("jsonl"));
        assert!(json.contains("input_hash_asc"));
        // None fields should be skipped
        assert!(!json.contains("column_mapping"));
    }

    #[test]
    fn test_build_determinism_ordering() {
        // Build same dataset twice in different initial orders
        // and verify the output ordering is identical
        let dir = tempdir().unwrap();

        // Create test JSONL files with predictable content
        create_test_jsonl(
            dir.path(),
            "data.jsonl",
            &[
                r#"{"input":"What is Rust?","output":"A systems programming language."}"#,
                r#"{"input":"What is Python?","output":"A high-level programming language."}"#,
                r#"{"input":"What is Go?","output":"A compiled language by Google."}"#,
            ],
        );

        // Parse and sort twice - ordering should be identical
        let parser_config = ParserConfig {
            column_mapping: None,
            text_strategy: TextStrategy::ParagraphPairs,
        };

        let samples1 = parse_file(
            dir.path().join("data.jsonl").as_path(),
            DatasetFormat::Jsonl,
            &parser_config,
        )
        .unwrap();
        let mut samples1: Vec<_> = samples1;
        deterministic_sort(&mut samples1);
        let hashes1: Vec<_> = samples1
            .iter()
            .map(|s| blake3::hash(s.input.as_bytes()).to_hex().to_string())
            .collect();

        // Parse again and sort
        let samples2 = parse_file(
            dir.path().join("data.jsonl").as_path(),
            DatasetFormat::Jsonl,
            &parser_config,
        )
        .unwrap();
        let mut samples2: Vec<_> = samples2;
        deterministic_sort(&mut samples2);
        let hashes2: Vec<_> = samples2
            .iter()
            .map(|s| blake3::hash(s.input.as_bytes()).to_hex().to_string())
            .collect();

        // Verify identical ordering
        assert_eq!(hashes1, hashes2);
        assert_eq!(samples1.len(), samples2.len());
        for (a, b) in samples1.iter().zip(samples2.iter()) {
            assert_eq!(a.input, b.input);
            assert_eq!(a.target, b.target);
        }
    }

    #[test]
    fn test_ordering_stability_across_file_order() {
        // Create dataset from files added in different orders
        // Output should be identical regardless of file creation order
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        let sample_a = r#"{"input":"alpha","output":"first letter"}"#;
        let sample_b = r#"{"input":"beta","output":"second letter"}"#;
        let sample_c = r#"{"input":"gamma","output":"third letter"}"#;

        // Directory 1: create files in order a, b, c
        create_test_jsonl(dir1.path(), "a.jsonl", &[sample_a]);
        create_test_jsonl(dir1.path(), "b.jsonl", &[sample_b]);
        create_test_jsonl(dir1.path(), "c.jsonl", &[sample_c]);

        // Directory 2: create files in order c, a, b
        create_test_jsonl(dir2.path(), "c.jsonl", &[sample_c]);
        create_test_jsonl(dir2.path(), "a.jsonl", &[sample_a]);
        create_test_jsonl(dir2.path(), "b.jsonl", &[sample_b]);

        // Collect and sort files
        let files1 = collect_files_sorted(dir1.path()).unwrap();
        let files2 = collect_files_sorted(dir2.path()).unwrap();

        // Files should be in same alphabetical order
        assert_eq!(files1.len(), files2.len());
        for (f1, f2) in files1.iter().zip(files2.iter()) {
            assert_eq!(f1.file_name(), f2.file_name());
        }

        // Parse all samples and sort
        let parser_config = ParserConfig {
            column_mapping: None,
            text_strategy: TextStrategy::ParagraphPairs,
        };

        let mut all_samples1: Vec<RawSample> = Vec::new();
        for file in &files1 {
            all_samples1.extend(parse_file(file, DatasetFormat::Jsonl, &parser_config).unwrap());
        }
        deterministic_sort(&mut all_samples1);

        let mut all_samples2: Vec<RawSample> = Vec::new();
        for file in &files2 {
            all_samples2.extend(parse_file(file, DatasetFormat::Jsonl, &parser_config).unwrap());
        }
        deterministic_sort(&mut all_samples2);

        // Ordering should be identical
        assert_eq!(all_samples1.len(), all_samples2.len());
        for (a, b) in all_samples1.iter().zip(all_samples2.iter()) {
            assert_eq!(a.input, b.input);
            assert_eq!(a.target, b.target);
        }
    }

    #[test]
    fn test_normalization_idempotent() {
        use super::super::normalize::normalize_text;

        // Various inputs that need normalization
        let inputs = [
            "Hello\r\nWorld",      // CRLF
            "Hello\rWorld",        // CR only
            "  trailing space   ", // trailing whitespace
            "line1  \nline2  ",    // mixed
            "already\nnormal",     // already normalized
        ];

        for input in inputs {
            let normalized1 = normalize_text(input).unwrap();
            let normalized2 = normalize_text(&normalized1).unwrap();

            // Second normalization should not change anything
            assert_eq!(
                normalized1, normalized2,
                "Normalization should be idempotent for input: {:?}",
                input
            );
        }
    }

    #[test]
    fn test_hash_stability() {
        // Same content should always produce same hash
        let dir = tempdir().unwrap();
        let path = dir.path().join("stable.txt");

        let content = "deterministic content for hashing";
        fs::write(&path, content).unwrap();

        let hash1 = compute_file_hash(&path).unwrap();

        // Write same content again
        fs::write(&path, content).unwrap();
        let hash2 = compute_file_hash(&path).unwrap();

        // Different content should produce different hash
        fs::write(&path, "different content").unwrap();
        let hash3 = compute_file_hash(&path).unwrap();

        assert_eq!(hash1, hash2, "Same content should produce same hash");
        assert_ne!(
            hash1, hash3,
            "Different content should produce different hash"
        );
    }
}
