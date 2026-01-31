//! Guided dataset upload wizard for training data.
//!
//! Provides client-side validation and previews for the supported
//! ingestion paths:
//! - Manifest + JSONL (prompt/response with weights)
//! - Direct CSV with column mapping
//! - Direct text/markdown with simple pairing strategies

#[cfg(target_arch = "wasm32")]
use crate::api::ApiClient;
use crate::api::DatasetManifest;
use crate::components::spinner::SpinnerSize;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, FormField, Input, Spinner,
};
use uuid::Uuid;
use adapteros_api_types::TRAINING_DATA_CONTRACT_VERSION;
#[cfg(target_arch = "wasm32")]
use gloo_file::futures::read_as_text;
#[cfg(target_arch = "wasm32")]
use gloo_file::Blob;
use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[cfg(target_arch = "wasm32")]
const MANIFEST_MIME: &[&str] = &["application/json"];
#[cfg(target_arch = "wasm32")]
const JSONL_MIME: &[&str] = &["application/jsonl", "application/json", "text/plain"];
#[cfg(target_arch = "wasm32")]
const CSV_MIME: &[&str] = &["text/csv", "application/csv"];
#[cfg(target_arch = "wasm32")]
const TEXT_MIME: &[&str] = &["text/plain"];

#[cfg(target_arch = "wasm32")]
fn validate_file(
    file: &web_sys::File,
    max_bytes: u64,
    allowed_mime: &[&str],
    allowed_exts: &[&str],
    label: &str,
) -> Result<(), String> {
    let size = file.size() as u64;
    if size == 0 {
        return Err(format!("{} file is empty", label));
    }
    if size > max_bytes {
        return Err(format!(
            "{} is too large ({} bytes > {} byte limit)",
            label, size, max_bytes
        ));
    }

    let mime = file.type_();
    let name = file.name().to_lowercase();
    let mime_ok = mime.is_empty() || allowed_mime.iter().any(|m| mime == *m);
    let ext_ok = allowed_exts
        .iter()
        .any(|ext| name.ends_with(&ext.to_lowercase()));

    if !mime_ok && !ext_ok {
        return Err(format!(
            "{} has unsupported type '{}'; allowed: {}",
            label,
            if mime.is_empty() {
                "unknown"
            } else {
                mime.as_str()
            },
            allowed_exts.join(", ")
        ));
    }

    Ok(())
}

/// Preview row built from user-supplied files.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedRow {
    pub prompt: String,
    pub response: String,
    pub weight: f32,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvMapping {
    pub input_col: String,
    pub target_col: String,
    pub weight_col: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadMode {
    ManifestJsonl,
    Csv,
    Text,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStrategy {
    Echo,
    PairAdjacent,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedFile {
    name: String,
    text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DatasetUploadOutcome {
    pub dataset_id: String,
    pub dataset_version_id: Option<String>,
    pub dataset_hash_b3: Option<String>,
    pub sample_count: usize,
}

const PREVIEW_LIMIT: usize = 25;
const DEFAULT_LIMIT_MAX_FILES: usize = 1000;
const DEFAULT_LIMIT_MAX_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const DEFAULT_LIMIT_MAX_SAMPLES: usize = 100_000;
const DEFAULT_LIMIT_MAX_TOKENS: u64 = 100_000_000;

fn trim_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

fn parse_weight(raw: Option<&str>, line_no: usize, errors: &mut Vec<String>) -> f32 {
    if let Some(val) = raw.and_then(|w| w.parse::<f32>().ok()) {
        if val > 0.0 {
            return val;
        }
        errors.push(format!("Line {}: weight must be > 0", line_no));
    }
    1.0
}

fn guard_prompt_response(
    prompt: &str,
    response: &str,
    line_no: usize,
    errors: &mut Vec<String>,
) -> bool {
    if prompt.trim().is_empty() {
        errors.push(format!("Line {}: input/prompt is required", line_no));
        return false;
    }
    if response.trim().is_empty() {
        errors.push(format!("Line {}: target/response is required", line_no));
        return false;
    }
    true
}

/// Parse JSONL rows with prompt/response + optional weight.
pub fn parse_jsonl_rows(content: &str, file_name: &str) -> Result<Vec<ParsedRow>, Vec<String>> {
    let mut rows = Vec::new();
    let mut errors = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        if rows.len() >= PREVIEW_LIMIT {
            break;
        }
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("Line {}: invalid JSON ({})", line_no, e));
                continue;
            }
        };
        let Some(obj) = value.as_object() else {
            errors.push(format!("Line {}: expected JSON object", line_no));
            continue;
        };

        let prompt = obj
            .get("prompt")
            .or_else(|| obj.get("input"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let response = obj
            .get("response")
            .or_else(|| obj.get("target"))
            .or_else(|| obj.get("output"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if !guard_prompt_response(prompt, response, line_no, &mut errors) {
            continue;
        }

        let weight_raw = match obj.get("weight") {
            Some(v) if v.is_number() => v.as_f64().map(|n| n.to_string()),
            Some(v) => v.as_str().map(str::to_string),
            None => None,
        };
        let weight = parse_weight(weight_raw.as_deref(), line_no, &mut errors);

        rows.push(ParsedRow {
            prompt: prompt.to_string(),
            response: response.to_string(),
            weight,
            source: format!("{}#L{}", file_name, line_no),
        });
    }

    if rows.is_empty() && errors.is_empty() {
        errors.push("No valid rows found in JSONL file".to_string());
    }

    if errors.is_empty() {
        Ok(rows)
    } else {
        Err(errors)
    }
}

/// Parse CSV rows using the provided column mapping.
pub fn parse_csv_rows(
    content: &str,
    mapping: &CsvMapping,
    file_name: &str,
) -> Result<Vec<ParsedRow>, Vec<String>> {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_bytes());

    let headers = reader.headers().map_err(|e| vec![e.to_string()])?.clone();
    let input_idx = headers
        .iter()
        .position(|h| h == mapping.input_col.as_str())
        .ok_or_else(|| vec![format!("Missing input column '{}'", mapping.input_col)])?;
    let target_idx = headers
        .iter()
        .position(|h| h == mapping.target_col.as_str())
        .ok_or_else(|| vec![format!("Missing target column '{}'", mapping.target_col)])?;
    let weight_idx = mapping
        .weight_col
        .as_ref()
        .and_then(|name| headers.iter().position(|h| h == name));

    for (idx, record) in reader.records().enumerate() {
        if rows.len() >= PREVIEW_LIMIT {
            break;
        }
        let line_no = idx + 2; // account for header
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Line {}: {}", line_no, e));
                continue;
            }
        };
        let prompt = record.get(input_idx).unwrap_or("").trim();
        let response = record.get(target_idx).unwrap_or("").trim();
        if !guard_prompt_response(prompt, response, line_no, &mut errors) {
            continue;
        }
        let weight = weight_idx
            .and_then(|i| record.get(i))
            .map(|w| w.trim())
            .filter(|w| !w.is_empty());
        let weight_value = parse_weight(weight, line_no, &mut errors);
        rows.push(ParsedRow {
            prompt: prompt.to_string(),
            response: response.to_string(),
            weight: weight_value,
            source: format!("{}#L{}", file_name, line_no),
        });
    }

    if rows.is_empty() && errors.is_empty() {
        errors.push("No valid rows found in CSV file".to_string());
    }

    if errors.is_empty() {
        Ok(rows)
    } else {
        Err(errors)
    }
}

/// Parse text or markdown using the selected strategy.
pub fn parse_text_rows(
    content: &str,
    strategy: TextStrategy,
    file_name: &str,
) -> Result<Vec<ParsedRow>, Vec<String>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let blocks: Vec<&str> = content
        .split("\n\n")
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .collect();

    if strategy == TextStrategy::PairAdjacent && !blocks.len().is_multiple_of(2) {
        errors
            .push("Uneven number of blocks for paired text; last block has no target".to_string());
    }

    match strategy {
        TextStrategy::Echo => {
            for (idx, block) in blocks.into_iter().enumerate() {
                if rows.len() >= PREVIEW_LIMIT {
                    break;
                }
                rows.push(ParsedRow {
                    prompt: block.to_string(),
                    response: block.to_string(),
                    weight: 1.0,
                    source: format!("{}#{}", file_name, idx + 1),
                });
            }
        }
        TextStrategy::PairAdjacent => {
            let mut index = 0;
            while index + 1 < blocks.len() && rows.len() < PREVIEW_LIMIT {
                let prompt = blocks[index];
                let response = blocks[index + 1];
                if guard_prompt_response(prompt, response, index + 1, &mut errors) {
                    rows.push(ParsedRow {
                        prompt: prompt.to_string(),
                        response: response.to_string(),
                        weight: 1.0,
                        source: format!("{}#{}", file_name, index / 2 + 1),
                    });
                }
                index += 2;
            }
        }
    }

    if rows.is_empty() && errors.is_empty() {
        errors.push("No usable text blocks found".to_string());
    }

    if errors.is_empty() {
        Ok(rows)
    } else {
        Err(errors)
    }
}

#[component]
pub fn DatasetUploadWizard(
    open: RwSignal<bool>,
    on_complete: Callback<DatasetUploadOutcome>,
) -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &on_complete;

    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let idempotency_key = RwSignal::new(String::new());
    let mode = RwSignal::new(UploadMode::ManifestJsonl);
    let text_strategy = RwSignal::new(TextStrategy::Echo);
    let csv_headers = RwSignal::new(Vec::<String>::new());
    let csv_mapping = RwSignal::new(None::<CsvMapping>);
    let manifest_info = RwSignal::new(None::<DatasetManifest>);
    let preview_rows = RwSignal::new(Vec::<ParsedRow>::new());
    let parse_errors = RwSignal::new(Vec::<String>::new());
    let submitting = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    let upload_error = RwSignal::new(None::<String>);
    let upload_limits = (
        DEFAULT_LIMIT_MAX_FILES,
        DEFAULT_LIMIT_MAX_BYTES,
        DEFAULT_LIMIT_MAX_SAMPLES,
        DEFAULT_LIMIT_MAX_TOKENS,
    );

    #[cfg(target_arch = "wasm32")]
    let manifest_file: RwSignal<Option<SendWrapper<web_sys::File>>> =
        RwSignal::new(None::<SendWrapper<web_sys::File>>);
    #[cfg(target_arch = "wasm32")]
    let data_file: RwSignal<Option<SendWrapper<web_sys::File>>> =
        RwSignal::new(None::<SendWrapper<web_sys::File>>);
    #[cfg(target_arch = "wasm32")]
    let manifest_preview: RwSignal<Option<SelectedFile>> = RwSignal::new(None);
    #[cfg(target_arch = "wasm32")]
    let data_preview: RwSignal<Option<SelectedFile>> = RwSignal::new(None);

    let close: Callback<()> = Callback::new(move |_| {
        open.set(false);
        submitting.set(false);
        parse_errors.set(Vec::new());
        preview_rows.set(Vec::new());
        upload_error.set(None);
        status.set(String::new());
        idempotency_key.set(String::new());
    });

    let refresh_preview: Callback<()> = {
        #[cfg(target_arch = "wasm32")]
        let mode = mode.clone();
        #[cfg(target_arch = "wasm32")]
        let csv_mapping = csv_mapping.clone();
        #[cfg(target_arch = "wasm32")]
        let csv_headers = csv_headers.clone();
        let preview_rows = preview_rows.clone();
        let parse_errors = parse_errors.clone();
        let manifest_info = manifest_info.clone();
        #[cfg(target_arch = "wasm32")]
        let text_strategy = text_strategy.clone();
        let status = status.clone();
        #[cfg(target_arch = "wasm32")]
        let name = name.clone();
        #[cfg(target_arch = "wasm32")]
        let manifest_preview = manifest_preview.clone();
        #[cfg(target_arch = "wasm32")]
        let data_preview = data_preview.clone();

        Callback::new(move |_| {
            parse_errors.set(Vec::new());
            preview_rows.set(Vec::new());
            manifest_info.set(None);
            status.set(String::new());
            #[cfg(target_arch = "wasm32")]
            {
                let mode_value = mode.get();
                let csv_map_value = csv_mapping.get();
                let csv_headers_value = csv_headers.get();
                let text_strategy_value = text_strategy.get();
                let parse_errors = parse_errors.clone();
                let preview_rows = preview_rows.clone();
                let manifest_info = manifest_info.clone();
                let name = name.clone();
                match mode_value {
                    UploadMode::ManifestJsonl => {
                        let manifest_file_value = manifest_preview.get();
                        let data_file_value = data_preview.get();
                        if let (Some(manifest_file), Some(jsonl_file)) =
                            (manifest_file_value, data_file_value)
                        {
                            match serde_json::from_str::<DatasetManifest>(&manifest_file.text) {
                                Ok(manifest) => {
                                    if manifest.training_contract_version
                                        != TRAINING_DATA_CONTRACT_VERSION
                                    {
                                        parse_errors.set(vec![format!(
                                            "training_contract_version must be {}",
                                            TRAINING_DATA_CONTRACT_VERSION
                                        )]);
                                        return;
                                    }
                                    manifest_info.set(Some(manifest));
                                }
                                Err(e) => {
                                    parse_errors.set(vec![format!("Invalid manifest JSON: {}", e)]);
                                    return;
                                }
                            }

                            let parsed = parse_jsonl_rows(&jsonl_file.text, &jsonl_file.name);
                            match parsed {
                                Ok(rows) => {
                                    if !rows.is_empty() && name.get().is_empty() {
                                        name.set(format!("training-{}", jsonl_file.name));
                                    }
                                    preview_rows.set(rows);
                                }
                                Err(errs) => parse_errors.set(errs),
                            }
                        }
                    }
                    UploadMode::Csv => {
                        if let Some(file) = data_preview.get() {
                            let csv_text = file.text.clone();
                            if csv_map_value.is_none() {
                                let mut reader = csv::ReaderBuilder::new()
                                    .has_headers(true)
                                    .from_reader(csv_text.as_bytes());
                                if let Ok(headers) = reader.headers() {
                                    let header_names: Vec<String> =
                                        headers.iter().map(ToString::to_string).collect();
                                    csv_headers.set(header_names.clone());
                                    if header_names.len() >= 2 {
                                        csv_mapping.set(Some(CsvMapping {
                                            input_col: header_names[0].clone(),
                                            target_col: header_names[1].clone(),
                                            weight_col: header_names.get(2).cloned(),
                                        }));
                                    }
                                }
                            } else {
                                csv_headers.set(csv_headers_value.clone());
                            }

                            if let Some(mapping) = csv_mapping.get() {
                                match parse_csv_rows(&csv_text, &mapping, &file.name) {
                                    Ok(rows) => {
                                        preview_rows.set(rows);
                                        if name.get().is_empty() {
                                            name.set(format!("csv-{}", file.name));
                                        }
                                    }
                                    Err(errs) => parse_errors.set(errs),
                                }
                            }
                        }
                    }
                    UploadMode::Text => {
                        if let Some(file) = data_preview.get() {
                            match parse_text_rows(&file.text, text_strategy_value, &file.name) {
                                Ok(rows) => {
                                    preview_rows.set(rows);
                                    if name.get().is_empty() {
                                        name.set(format!("text-{}", file.name));
                                    }
                                }
                                Err(errs) => parse_errors.set(errs),
                            }
                        }
                    }
                }
            }
        })
    };

    #[cfg(target_arch = "wasm32")]
    let handle_manifest_file = {
        let manifest_file = manifest_file.clone();
        let manifest_preview = manifest_preview.clone();
        let parse_errors = parse_errors.clone();
        let refresh_preview = refresh_preview.clone();
        let upload_limits = upload_limits;
        move |ev: web_sys::Event| {
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        if let Err(err) = validate_file(
                            &file,
                            upload_limits.1,
                            MANIFEST_MIME,
                            &[".json"],
                            "manifest",
                        ) {
                            parse_errors.set(vec![err]);
                            return;
                        }

                        let parse_errors = parse_errors.clone();
                        let refresh_preview = refresh_preview.clone();
                        let name = file.name();
                        let manifest_file = manifest_file.clone();
                        let manifest_preview = manifest_preview.clone();
                        spawn_local(async move {
                            match read_as_text(&Blob::from(file.clone())).await {
                                Ok(text) => {
                                    manifest_file.set(Some(SendWrapper::new(file)));
                                    manifest_preview.set(Some(SelectedFile { name, text }));
                                    refresh_preview.run(());
                                }
                                Err(e) => {
                                    parse_errors
                                        .set(vec![format!("Failed to read manifest: {}", e)]);
                                }
                            }
                        });
                    }
                }
            }
        }
    };

    #[cfg(target_arch = "wasm32")]
    let handle_data_file = {
        let data_file = data_file.clone();
        let data_preview = data_preview.clone();
        let parse_errors = parse_errors.clone();
        let refresh_preview = refresh_preview.clone();
        let mode = mode.clone();
        let upload_limits = upload_limits;
        move |ev: web_sys::Event| {
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        let (allowed_mime, allowed_ext, label) = match mode.get() {
                            UploadMode::ManifestJsonl => {
                                (JSONL_MIME, &[".jsonl", ".json"][..], "JSONL dataset")
                            }
                            UploadMode::Csv => (CSV_MIME, &[".csv"][..], "CSV dataset"),
                            UploadMode::Text => {
                                (TEXT_MIME, &[".txt", ".log", ".md"][..], "text dataset")
                            }
                        };

                        if let Err(err) =
                            validate_file(&file, upload_limits.1, allowed_mime, allowed_ext, label)
                        {
                            parse_errors.set(vec![err]);
                            return;
                        }

                        let parse_errors = parse_errors.clone();
                        let refresh_preview = refresh_preview.clone();
                        let name = file.name();
                        let data_file = data_file.clone();
                        let data_preview = data_preview.clone();
                        spawn_local(async move {
                            match read_as_text(&Blob::from(file.clone())).await {
                                Ok(text) => {
                                    data_file.set(Some(SendWrapper::new(file)));
                                    data_preview.set(Some(SelectedFile { name, text }));
                                    refresh_preview.run(());
                                }
                                Err(e) => {
                                    parse_errors
                                        .set(vec![format!("Failed to read dataset: {}", e)]);
                                }
                            }
                        });
                    }
                }
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_manifest_file = |_ev: web_sys::Event| {};

    #[cfg(not(target_arch = "wasm32"))]
    let handle_data_file = |_ev: web_sys::Event| {};

    let on_upload: Callback<()> = {
        #[cfg(target_arch = "wasm32")]
        let manifest_file = manifest_file.clone();
        #[cfg(target_arch = "wasm32")]
        let data_file = data_file.clone();
        #[cfg(target_arch = "wasm32")]
        let name = name.clone();
        #[cfg(target_arch = "wasm32")]
        let description = description.clone();
        #[cfg(target_arch = "wasm32")]
        let idempotency_key = idempotency_key.clone();
        #[cfg(target_arch = "wasm32")]
        let mode = mode.clone();
        #[cfg(target_arch = "wasm32")]
        let csv_mapping = csv_mapping.clone();
        let preview_rows = preview_rows.clone();
        let parse_errors = parse_errors.clone();
        let status = status.clone();
        let upload_error = upload_error.clone();
        let submitting = submitting.clone();
        #[cfg(target_arch = "wasm32")]
        let manifest_info = manifest_info.clone();
        Callback::new(move |_| {
            upload_error.set(None);
            status.set(String::new());
            if preview_rows.get().is_empty() {
                parse_errors.set(vec!["Add at least one valid sample before uploading".into()]);
                return;
            }

            submitting.set(true);
            #[cfg(target_arch = "wasm32")]
            let dataset_name = if name.get().is_empty() {
                "training-dataset".to_string()
            } else {
                name.get()
            };
            #[cfg(target_arch = "wasm32")]
            let description_value = description.get();
            #[cfg(target_arch = "wasm32")]
            {
                let client = ApiClient::new();
                let data_file_value = data_file.get();
                let manifest_value = manifest_file.get();
                let mode_value = mode.get();
                let csv_mapping = csv_mapping.get();
                let manifest_value_cached = manifest_info.get();
                status.set("Uploading dataset (this may take a moment)...".to_string());
                spawn_local(async move {
                    let form = match web_sys::FormData::new() {
                        Ok(f) => f,
                        Err(_) => {
                            upload_error.set(Some("Failed to create upload form".into()));
                            submitting.set(false);
                            return;
                        }
                    };
                    let format = match mode_value {
                        UploadMode::ManifestJsonl => "jsonl",
                        UploadMode::Csv => "csv",
                        UploadMode::Text => "txt",
                    };
                    let Some(data_file) = data_file_value.map(|file| file.take()) else {
                        upload_error.set(Some("Select a dataset file to upload".into()));
                        submitting.set(false);
                        return;
                    };
                    if let Err(_) = form.append_with_blob("files[]", data_file.as_ref()) {
                        upload_error.set(Some("Failed to attach dataset file".into()));
                        submitting.set(false);
                        return;
                    }
                    if let UploadMode::ManifestJsonl = mode_value {
                        if let Some(manifest) = manifest_value.map(|file| file.take()) {
                            if let Err(_) = form.append_with_blob("files[]", manifest.as_ref()) {
                                upload_error.set(Some("Failed to attach manifest file".into()));
                                submitting.set(false);
                                return;
                            }
                        } else {
                            upload_error.set(Some("Manifest is required for this format".into()));
                            submitting.set(false);
                            return;
                        }
                    }
                    form.append_with_str("name", &dataset_name).ok();
                    form.append_with_str("format", format).ok();
                    if !description_value.is_empty() {
                        form.append_with_str("description", &description_value).ok();
                    }
                    if let (UploadMode::Csv, Some(mapping)) = (mode_value, csv_mapping) {
                        form.append_with_str("metadata[csv_input]", &mapping.input_col)
                            .ok();
                        form.append_with_str("metadata[csv_target]", &mapping.target_col)
                            .ok();
                        if let Some(weight) = mapping.weight_col {
                            form.append_with_str("metadata[csv_weight]", &weight).ok();
                        }
                    }

                    let idempotency_value = idempotency_key.get();
                    let idempotency_header = if idempotency_value.trim().is_empty() {
                        None
                    } else {
                        Some(idempotency_value.trim().to_string())
                    };

                    match client
                        .upload_dataset(&form, idempotency_header.as_deref())
                        .await
                    {
                        Ok(resp) => {
                            status.set(format!(
                                "Dataset {} uploaded ({} files, {} bytes)",
                                resp.dataset_id, resp.file_count, resp.total_size_bytes
                            ));
                            let version_id = resp.dataset_version_id.clone();
                            let hash = resp.dataset_hash_b3.clone();
                            let sample_count = match (version_id.clone(), manifest_value_cached) {
                                (Some(ver_id), Some(manifest))
                                    if manifest.dataset_version_id == ver_id =>
                                {
                                    manifest.total_rows
                                }
                                _ => preview_rows.get().len(),
                            };
                            on_complete.run(DatasetUploadOutcome {
                                dataset_id: resp.dataset_id.clone(),
                                dataset_version_id: version_id,
                                dataset_hash_b3: hash,
                                sample_count,
                            });
                            submitting.set(false);
                            open.set(false);
                        }
                        Err(e) => {
                            upload_error.set(Some(e.to_string()));
                            submitting.set(false);
                        }
                    }
                });
            }
        })
    };

    let generate_idempotency_key = Callback::new(move |_| {
        idempotency_key.set(Uuid::new_v4().to_string());
    });

    let dialog = move || -> AnyView {
        let manifest_handler = handle_manifest_file.clone();
        let data_handler = handle_data_file.clone();
        if !open.get() {
            view! {}.into_any()
        } else {
            view! {
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70" on:click=move |_| close.run(())/>
                <div class="dialog-content dialog-scrollable max-w-5xl">
                    <div class="flex items-center justify-between mb-4">
                        <div>
                            <h2 class="text-lg font-semibold">"Upload Training Dataset"</h2>
                            <p class="text-sm text-muted-foreground">
                                "Pick a format, validate required fields, and preview the parsed samples before upload."
                            </p>
                        </div>
                        <Button variant=ButtonVariant::Ghost on_click=Callback::new(move |_| close.run(()))>"Close"</Button>
                    </div>

                    <div class="grid gap-6 grid-cols-1 md:grid-cols-3">
                        <div class="space-y-4 md:col-span-1">
                            <FormField label="Dataset Name" name="dataset_name" required=false>
                                <Input value=name placeholder="my-training-data".to_string()/>
                            </FormField>
                            <FormField label="Description" name="description" required=false>
                                <textarea
                                    class="flex min-h-[80px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                                    prop:value=move || description.get()
                                    on:input=move |ev| description.set(event_target_value(&ev))
                                    placeholder="What is in this dataset?"
                                />
                            </FormField>
                            <FormField label="Idempotency Key (optional)" name="idempotency_key" required=false>
                                <Input
                                    value=idempotency_key
                                    placeholder="uuid-or-unique-key".to_string()
                                />
                            </FormField>
                            <div class="flex items-center justify-between text-xs text-muted-foreground">
                                <span>"Reuse the same key to safely retry an upload."</span>
                                <Button
                                    variant=ButtonVariant::Ghost
                                    size=ButtonSize::Sm
                                    on_click=generate_idempotency_key
                                >
                                    "Generate"
                                </Button>
                            </div>

                            <div class="space-y-2">
                                <label class="text-sm font-medium">"Format"</label>
                                <div class="flex gap-2">
                                    {vec![
                                        (UploadMode::ManifestJsonl, "Manifest + JSONL"),
                                        (UploadMode::Csv, "CSV"),
                                        (UploadMode::Text, "Text / Markdown"),
                                    ].into_iter().map(|(value, label)| {
                                        let mode_signal = mode.clone();
                                        let refresh = refresh_preview.clone();
                                        view! {
                                            <button
                                                class=move || {
                                                    if mode_signal.get() == value {
                                                        "px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm"
                                                    } else {
                                                        "px-3 py-2 rounded-md border text-sm"
                                                    }
                                                }
                                                on:click=move |_| {
                                                    mode_signal.set(value);
                                                    refresh.run(());
                                                }
                                            >
                                                {label}
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                                <p class="text-xs text-muted-foreground">
                                    "Required fields: prompt/input, target/response, optional weight > 0. "
                                    "Manifest must match TRAINING_DATA_CONTRACT_VERSION=" {TRAINING_DATA_CONTRACT_VERSION}
                                    "."
                                </p>
                                <div class="text-xs text-muted-foreground space-y-1">
                                    <div class="font-medium text-foreground">"Size guardrails"</div>
                                    <div>{format!("Files: <= {}", upload_limits.0)}</div>
                                    <div>{format!("Total bytes: <= {:.1} GiB", upload_limits.1 as f64 / 1024.0 / 1024.0 / 1024.0)}</div>
                                    <div>{format!("Samples: <= {}", upload_limits.2)}</div>
                                    <div>{format!("Tokens (server enforced): <= {}", upload_limits.3)}</div>
                                </div>
                            </div>
                        </div>

                        <div class="space-y-4 md:col-span-2">
                            {move || match mode.get() {
                            UploadMode::ManifestJsonl => view! {
                                <div class="rounded-lg border p-4 space-y-3">
                                    <div class="flex items-center justify-between">
                                        <div>
                                            <div class="text-sm font-medium">"Manifest + JSONL"</div>
                                            <p class="text-xs text-muted-foreground">
                                                "Manifest must include training_contract_version "
                                                {TRAINING_DATA_CONTRACT_VERSION} ", and the JSONL must contain prompt/response pairs."
                                            </p>
                                        </div>
                                        <Badge variant=BadgeVariant::Secondary>"Prompt / Response"</Badge>
                                    </div>
                                    <div class="grid gap-3 md:grid-cols-2">
                                        <div>
                                            <label class="text-sm font-medium">"Manifest (.json)"</label>
                                            <input type="file" accept=".json" class="mt-1 block w-full text-sm" on:change=manifest_handler.clone()/>
                                        </div>
                                        <div>
                                            <label class="text-sm font-medium">"Dataset (.jsonl)"</label>
                                            <input type="file" accept=".jsonl" class="mt-1 block w-full text-sm" on:change=data_handler.clone()/>
                                        </div>
                                    </div>
                                    {move || manifest_info.get().map(|m| view! {
                                        <div class="rounded-md bg-muted p-3 text-xs text-muted-foreground space-y-1">
                                            <div class="font-medium text-foreground">"Manifest summary"</div>
                                            <div>{format!("Dataset: {}", m.dataset_id)}</div>
                                            <div>{format!("Version: {}", m.dataset_version_id)}</div>
                                            <div>{format!("Total rows: {}", m.total_rows)}</div>
                                            <div>{format!("Hash: {}", m.hash_b3)}</div>
                                        </div>
                                    })}
                                </div>
                            }.into_any(),
                            UploadMode::Csv => view! {
                                    <div class="rounded-lg border p-4 space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <div class="text-sm font-medium">"CSV with column mapping"</div>
                                            <p class="text-xs text-muted-foreground">
                                                "Map your columns to input and target; optional weight column must be > 0."
                                            </p>
                                        </div>
                                        <Badge variant=BadgeVariant::Secondary>"Input / Target"</Badge>
                                    </div>
                                        <input type="file" accept=".csv" class="mt-1 block w-full text-sm" on:change=data_handler.clone()/>
                                    {move || {
                                        let headers = csv_headers.get();
                                        if headers.is_empty() {
                                            view! {}.into_any()
                                        } else {
                                            let mapping = csv_mapping.get().unwrap_or_else(|| CsvMapping {
                                                input_col: headers[0].clone(),
                                                target_col: headers.get(1).cloned().unwrap_or_default(),
                                                weight_col: headers.get(2).cloned(),
                                            });
                                            let mapping_for_input = mapping.clone();
                                            let mapping_for_target = mapping.clone();
                                            let mapping_for_weight = mapping.clone();
                                            let mapping_for_options = mapping.clone();
                                            let set_mapping = csv_mapping.clone();
                                            let refresh = refresh_preview.clone();
                                            view! {
                                                <div class="grid gap-3 md:grid-cols-3">
                                                    <div class="space-y-1">
                                                        <label class="text-sm font-medium">"Input column"</label>
                                                        <select
                                                            class="w-full rounded-md border px-2 py-2 text-sm"
                                                            on:change=move |ev| {
                                                                let value = event_target_value(&ev);
                                                                let mut m = mapping_for_input.clone();
                                                                m.input_col = value;
                                                                set_mapping.set(Some(m.clone()));
                                                                refresh.run(());
                                                            }
                                                        >
                                                            {headers.iter().map(|h| view! {
                                                                <option value=h.clone() selected={*h == mapping_for_options.input_col}>{h.clone()}</option>
                                                            }).collect_view()}
                                                        </select>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="text-sm font-medium">"Target column"</label>
                                                        <select
                                                            class="w-full rounded-md border px-2 py-2 text-sm"
                                                            on:change=move |ev| {
                                                                let value = event_target_value(&ev);
                                                                let mut m = mapping_for_target.clone();
                                                                m.target_col = value;
                                                                set_mapping.set(Some(m.clone()));
                                                                refresh.run(());
                                                            }
                                                        >
                                                            {headers.iter().map(|h| view! {
                                                                <option value=h.clone() selected={*h == mapping_for_options.target_col}>{h.clone()}</option>
                                                            }).collect_view()}
                                                        </select>
                                                    </div>
                                                    <div class="space-y-1">
                                                        <label class="text-sm font-medium">"Weight column (optional)"</label>
                                                        <select
                                                            class="w-full rounded-md border px-2 py-2 text-sm"
                                                            on:change=move |ev| {
                                                                let value = event_target_value(&ev);
                                                                let mut m = mapping_for_weight.clone();
                                                                m.weight_col = trim_opt(Some(&value));
                                                                set_mapping.set(Some(m.clone()));
                                                                refresh.run(());
                                                            }
                                                        >
                                                            <option value="">"None"</option>
                                                            {headers.iter().map(|h| view! {
                                                                <option value=h.clone() selected={Some(h) == mapping_for_options.weight_col.as_ref()}>{h.clone()}</option>
                                                            }).collect_view()}
                                                        </select>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }
                                    }}
                                </div>
                            }.into_any(),
                            UploadMode::Text => view! {
                                    <div class="rounded-lg border p-4 space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <div class="text-sm font-medium">"Text / Markdown"</div>
                                            <p class="text-xs text-muted-foreground">
                                                "Choose how to pair blocks: echo uses the same text for input and target; pairing consumes adjacent blocks."
                                            </p>
                                        </div>
                                        <Badge variant=BadgeVariant::Secondary>"Input / Target"</Badge>
                                    </div>
                                    <input type="file" accept=".txt,.md,.markdown" class="mt-1 block w-full text-sm" on:change=data_handler.clone()/>
                                    <div class="flex gap-2">
                                        {vec![
                                            (TextStrategy::Echo, "Echo input as target"),
                                            (TextStrategy::PairAdjacent, "Pair adjacent blocks"),
                                        ].into_iter().map(|(value, label)| {
                                            let text_strategy = text_strategy.clone();
                                            let refresh = refresh_preview.clone();
                                            view! {
                                                <button
                                                    class=move || {
                                                        if text_strategy.get() == value {
                                                            "px-3 py-2 rounded-md bg-primary text-primary-foreground text-xs"
                                                        } else {
                                                            "px-3 py-2 rounded-md border text-xs"
                                                        }
                                                    }
                                                    on:click=move |_| {
                                                        text_strategy.set(value);
                                                        refresh.run(());
                                                    }
                                                >
                                                    {label}
                                                </button>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>
                            }.into_any(),
                        }}

                        {move || {
                            let errors = parse_errors.get();
                            if errors.is_empty() {
                                view! {}.into_any()
                            } else {
                                view! {
                                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive space-y-1">
                                        {errors.into_iter().map(|e| view! { <div>{e}</div> }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }}

                        {move || {
                            let rows = preview_rows.get();
                            if rows.is_empty() {
                                view! {
                                    <div class="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                                        "Add files to preview the first parsed samples (enforces non-empty input/target, weight > 0)."
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="rounded-lg border p-3 space-y-2">
                                        <div class="flex items-center justify-between text-sm">
                                            <span class="font-medium">"Preview"</span>
                                            <span class="text-muted-foreground">{format!("Showing {} of {} samples", rows.len().min(PREVIEW_LIMIT), rows.len())}</span>
                                        </div>
                                        <div class="overflow-auto">
                                            <table class="w-full text-sm">
                                                <thead>
                                                    <tr class="text-left text-muted-foreground">
                                                        <th class="p-2">"Prompt/Input"</th>
                                                        <th class="p-2">"Target/Response"</th>
                                                        <th class="p-2 w-20">"Weight"</th>
                                                        <th class="p-2">"Provenance"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {rows.into_iter().take(PREVIEW_LIMIT).map(|row| view! {
                                                        <tr class="border-t">
                                                            <td class="p-2 align-top whitespace-pre-wrap">{row.prompt.clone()}</td>
                                                            <td class="p-2 align-top whitespace-pre-wrap">{row.response.clone()}</td>
                                                            <td class="p-2 align-top">{format!("{:.2}", row.weight)}</td>
                                                            <td class="p-2 align-top text-xs text-muted-foreground">{row.source.clone()}</td>
                                                        </tr>
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        }}

                        {move || upload_error.get().map(|err| view! {
                            <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                                {err}
                            </div>
                        })}
                        {move || {
                            let msg = status.get();
                            if msg.is_empty() {
                                view! {}.into_any()
                            } else {
                                view! {
                                    <div class="rounded-md border border-green-600/60 bg-green-100/40 p-3 text-sm text-foreground">
                                        {msg}
                                    </div>
                                }.into_any()
                            }
                        }}

                        <div class="flex justify-end gap-2">
                            <Button variant=ButtonVariant::Ghost on_click=Callback::new(move |_| close.run(()))>"Cancel"</Button>
                            <Button
                                variant=ButtonVariant::Primary
                                disabled=submitting.get()
                                on_click=on_upload
                            >
                                {move || if submitting.get() {
                                    view! { <div class="flex items-center gap-2"><Spinner size=SpinnerSize::Sm/> "Uploading..."</div> }.into_any()
                                } else {
                                    view! { "Upload dataset" }.into_any()
                                }}
                            </Button>
                        </div>
                    </div>
                </div>
                </div>
            }.into_any()
        }
    };

    view! {
        {dialog}
    }
}
