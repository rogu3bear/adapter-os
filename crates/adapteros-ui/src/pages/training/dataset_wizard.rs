//! Guided dataset upload wizard for training data.
//!
//! Provides client-side validation and previews for the supported
//! ingestion paths:
//! - JSONL (prompt/response with weights)
//! - Direct CSV with column mapping
//! - Direct text/markdown with simple pairing strategies

use crate::api::error::format_structured_details;
#[cfg(target_arch = "wasm32")]
use crate::api::ApiClient;
use crate::components::spinner::SpinnerSize;
use crate::components::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Card, Dialog, DialogSize, FormField,
    Input, Select, Spinner,
};
use crate::validation::{rules, use_form_errors, validate_field};
#[cfg(target_arch = "wasm32")]
use gloo_file::futures::read_as_text;
#[cfg(target_arch = "wasm32")]
use gloo_file::Blob;

fn readable_id(_prefix: &str, _slug_source: &str) -> String {
    adapteros_id::TypedId::new(adapteros_id::IdPrefix::Req).to_string()
}
use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

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
    // Individual signals for CSV column Select components
    let csv_input_col = RwSignal::new(String::new());
    let csv_target_col = RwSignal::new(String::new());
    let csv_weight_col = RwSignal::new(String::new());
    let preview_rows = RwSignal::new(Vec::<ParsedRow>::new());
    let parse_errors = RwSignal::new(Vec::<String>::new());
    let submitting = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    let upload_error = RwSignal::new(None::<String>);
    let form_errors = use_form_errors();
    let upload_limits = (
        DEFAULT_LIMIT_MAX_FILES,
        DEFAULT_LIMIT_MAX_BYTES,
        DEFAULT_LIMIT_MAX_SAMPLES,
        DEFAULT_LIMIT_MAX_TOKENS,
    );

    #[cfg(target_arch = "wasm32")]
    let data_file: RwSignal<Option<SendWrapper<web_sys::File>>> =
        RwSignal::new(None::<SendWrapper<web_sys::File>>);
    #[cfg(target_arch = "wasm32")]
    let data_preview: RwSignal<Option<SelectedFile>> = RwSignal::new(None);
    let is_active = Arc::new(AtomicBool::new(true));
    on_cleanup({
        let is_active = Arc::clone(&is_active);
        move || is_active.store(false, Ordering::Relaxed)
    });

    // Reset form when dialog closes (Effect-based, matches CreateJobWizard pattern)
    let reset_form = move || {
        submitting.set(false);
        parse_errors.set(Vec::new());
        preview_rows.set(Vec::new());
        upload_error.set(None);
        status.set(String::new());
        idempotency_key.set(String::new());
        name.set(String::new());
        description.set(String::new());
        mode.set(UploadMode::ManifestJsonl);
        text_strategy.set(TextStrategy::Echo);
        csv_headers.set(Vec::new());
        csv_mapping.set(None);
        csv_input_col.set(String::new());
        csv_target_col.set(String::new());
        csv_weight_col.set(String::new());
        form_errors.update(|e| e.clear_all());
        #[cfg(target_arch = "wasm32")]
        {
            data_file.set(None);
            data_preview.set(None);
        }
    };

    let was_open = StoredValue::new(open.get_untracked());
    Effect::new(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let prev = was_open.get_value();
        was_open.set_value(is_open);
        if prev && !is_open {
            reset_form();
        }
    });

    let refresh_preview: Callback<()> = {
        Callback::new(move |_| {
            parse_errors.set(Vec::new());
            preview_rows.set(Vec::new());
            status.set(String::new());
            #[cfg(target_arch = "wasm32")]
            {
                let mode_value = mode.get();
                let csv_map_value = csv_mapping.get();
                let csv_headers_value = csv_headers.get();
                let text_strategy_value = text_strategy.get();
                match mode_value {
                    UploadMode::ManifestJsonl => {
                        let data_file_value = data_preview.get();
                        if let Some(jsonl_file) = data_file_value {
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
                                        csv_input_col.set(header_names[0].clone());
                                        csv_target_col.set(header_names[1].clone());
                                        csv_weight_col
                                            .set(header_names.get(2).cloned().unwrap_or_default());
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
    let handle_data_file = {
        let is_active = Arc::clone(&is_active);
        move |ev: web_sys::Event| {
            if let Some(input) = ev
                .target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        let (allowed_mime, allowed_ext, label) = match mode.get() {
                            UploadMode::ManifestJsonl => {
                                (JSONL_MIME, &[".jsonl", ".ndjson"][..], "JSONL dataset")
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
                            input.set_value("");
                            return;
                        }

                        let name = file.name();
                        let is_active = Arc::clone(&is_active);
                        spawn_local(async move {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            match read_as_text(&Blob::from(file.clone())).await {
                                Ok(text) => {
                                    if !is_active.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    let _ = data_file.try_set(Some(SendWrapper::new(file)));
                                    let _ = data_preview.try_set(Some(SelectedFile { name, text }));
                                    refresh_preview.run(());
                                }
                                Err(e) => {
                                    if !is_active.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    let _ = parse_errors
                                        .try_set(vec![format!("Failed to read dataset: {}", e)]);
                                }
                            }
                        });
                        input.set_value("");
                    }
                }
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let handle_data_file = |_ev: web_sys::Event| {};
    let data_handler = Callback::new(move |ev: web_sys::Event| {
        handle_data_file(ev);
    });

    // Form-level validation before upload
    let validate_upload = move || -> bool {
        form_errors.update(|e| e.clear_all());
        let mut valid = true;

        if let Some(err) = validate_field(&name.get(), &rules::description()) {
            form_errors.update(|e| e.set("dataset_name", err));
            valid = false;
        }

        if preview_rows.get().is_empty() {
            form_errors.update(|e| {
                e.set(
                    "file",
                    "Add at least one valid sample before uploading".to_string(),
                )
            });
            valid = false;
        }

        valid
    };

    let on_upload: Callback<()> = {
        Callback::new(move |_| {
            upload_error.set(None);
            status.set(String::new());

            if !validate_upload() {
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
                let mode_value = mode.get();
                let csv_mapping = csv_mapping.get();
                let sample_count = preview_rows.get().len();
                let idempotency_value = idempotency_key.get();
                status.set("Uploading dataset (this may take a moment)...".to_string());
                let is_active = Arc::clone(&is_active);
                spawn_local(async move {
                    if !is_active.load(Ordering::Relaxed) {
                        return;
                    }
                    let form = match web_sys::FormData::new() {
                        Ok(f) => f,
                        Err(_) => {
                            let _ =
                                upload_error.try_set(Some("Failed to create upload form".into()));
                            let _ = submitting.try_set(false);
                            return;
                        }
                    };
                    let format = match mode_value {
                        UploadMode::ManifestJsonl => "jsonl",
                        UploadMode::Csv => "csv",
                        UploadMode::Text => "txt",
                    };
                    let Some(data_file) = data_file_value.map(|file| file.take()) else {
                        let _ =
                            upload_error.try_set(Some("Select a dataset file to upload".into()));
                        let _ = submitting.try_set(false);
                        return;
                    };
                    if let Err(_) = form.append_with_blob("files[]", data_file.as_ref()) {
                        let _ = upload_error.try_set(Some("Failed to attach dataset file".into()));
                        let _ = submitting.try_set(false);
                        return;
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
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = status.try_set(format!(
                                "Dataset {} uploaded ({} files, {} bytes)",
                                resp.dataset_id, resp.file_count, resp.total_size_bytes
                            ));
                            let version_id = resp.dataset_version_id.clone();
                            let hash = resp.dataset_hash_b3.clone();
                            on_complete.run(DatasetUploadOutcome {
                                dataset_id: resp.dataset_id.clone(),
                                dataset_version_id: version_id,
                                dataset_hash_b3: hash,
                                sample_count,
                            });
                            let _ = submitting.try_set(false);
                            let _ = open.try_set(false);
                        }
                        Err(e) => {
                            if !is_active.load(Ordering::Relaxed) {
                                return;
                            }
                            let _ = upload_error.try_set(Some(format_structured_details(&e)));
                            let _ = submitting.try_set(false);
                        }
                    }
                });
            }
        })
    };

    let generate_idempotency_key = Callback::new(move |_| {
        idempotency_key.set(readable_id("idem", "dataset"));
    });

    // CSV column change callbacks that sync individual signals → CsvMapping + refresh
    let on_csv_input_change = Callback::new(move |val: String| {
        csv_input_col.set(val.clone());
        let target = csv_target_col.get();
        let weight = trim_opt(Some(&csv_weight_col.get()));
        csv_mapping.set(Some(CsvMapping {
            input_col: val,
            target_col: target,
            weight_col: weight,
        }));
        refresh_preview.run(());
    });

    let on_csv_target_change = Callback::new(move |val: String| {
        csv_target_col.set(val.clone());
        let input = csv_input_col.get();
        let weight = trim_opt(Some(&csv_weight_col.get()));
        csv_mapping.set(Some(CsvMapping {
            input_col: input,
            target_col: val,
            weight_col: weight,
        }));
        refresh_preview.run(());
    });

    let on_csv_weight_change = Callback::new(move |val: String| {
        csv_weight_col.set(val.clone());
        let input = csv_input_col.get();
        let target = csv_target_col.get();
        csv_mapping.set(Some(CsvMapping {
            input_col: input,
            target_col: target,
            weight_col: trim_opt(Some(&val)),
        }));
        refresh_preview.run(());
    });

    let dialog = move || -> AnyView {
        if !open.try_get().unwrap_or(false) {
            view! {}.into_any()
        } else {
            view! {
                <Dialog
                    open=open
                    title="Upload Training Dataset".to_string()
                    description="Pick a format, validate required fields, and preview the parsed samples before upload.".to_string()
                    size=DialogSize::Xl
                    scrollable=true
                >
                    <div class="grid gap-6 grid-cols-1 md:grid-cols-3">
                        <div class="space-y-4 md:col-span-1">
                            <FormField
                                label="Dataset Name"
                                name="dataset_name"
                                required=false
                                error=Signal::derive(move || form_errors.try_get().unwrap_or_default().get("dataset_name").cloned())
                            >
                                <Input value=name placeholder="my-training-data".to_string()/>
                            </FormField>
                            <FormField label="Description" name="description" required=false>
                                <textarea
                                    class="input min-h-[80px] w-full"
                                    prop:value=move || description.try_get().unwrap_or_default()
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
                                        (UploadMode::ManifestJsonl, "JSONL"),
                                        (UploadMode::Csv, "CSV"),
                                        (UploadMode::Text, "Text / Markdown"),
                                    ].into_iter().map(|(value, label)| {
                                        view! {
                                            <button
                                                class=move || {
                                                    if mode.try_get().unwrap_or(UploadMode::ManifestJsonl) == value {
                                                        "px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm"
                                                    } else {
                                                        "px-3 py-2 rounded-md border text-sm"
                                                    }
                                                }
                                                on:click=move |_| {
                                                    mode.set(value);
                                                    refresh_preview.run(());
                                                }
                                            >
                                                {label}
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                                <p class="text-xs text-muted-foreground">
                                    "Required fields: prompt/input, target/response, optional weight > 0. "
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
                            {move || match mode.try_get().unwrap_or(UploadMode::ManifestJsonl) {
                            UploadMode::ManifestJsonl => view! {
                                <Card>
                                    <div class="p-4 space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <div class="text-sm font-medium">"JSONL"</div>
                                                <p class="text-xs text-muted-foreground">
                                                    "Each line must be a JSON object with prompt/response (or input/target)."
                                                </p>
                                            </div>
                                            <Badge variant=BadgeVariant::Secondary>"Prompt / Response"</Badge>
                                        </div>
                                        <div>
                                            <label class="text-sm font-medium">"Dataset (.jsonl or .ndjson)"</label>
                                            <input
                                                type="file"
                                                accept=".jsonl,.ndjson"
                                                class="mt-1 block w-full text-sm"
                                                on:change=move |ev| data_handler.run(ev)
                                            />
                                        </div>
                                    </div>
                                </Card>
                            }.into_any(),
                            UploadMode::Csv => view! {
                                <Card>
                                    <div class="p-4 space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <div class="text-sm font-medium">"CSV with column mapping"</div>
                                                <p class="text-xs text-muted-foreground">
                                                    "Map your columns to input and target; optional weight column must be > 0."
                                                </p>
                                            </div>
                                            <Badge variant=BadgeVariant::Secondary>"Input / Target"</Badge>
                                        </div>
                                        <input
                                            type="file"
                                            accept=".csv"
                                            class="mt-1 block w-full text-sm"
                                            on:change=move |ev| data_handler.run(ev)
                                        />
                                        {move || {
                                            let headers = csv_headers.try_get().unwrap_or_default();
                                            if headers.is_empty() {
                                                view! {}.into_any()
                                            } else {
                                                let input_options: Vec<(String, String)> = headers.iter()
                                                    .map(|h| (h.clone(), h.clone()))
                                                    .collect();
                                                let target_options = input_options.clone();
                                                let mut weight_options: Vec<(String, String)> = vec![
                                                    (String::new(), "None".to_string()),
                                                ];
                                                weight_options.extend(headers.iter().map(|h| (h.clone(), h.clone())));
                                                view! {
                                                    <div class="grid gap-3 md:grid-cols-3">
                                                        <FormField label="Input column" name="csv_input_col">
                                                            <Select
                                                                value=csv_input_col
                                                                options=input_options
                                                                on_change=on_csv_input_change
                                                            />
                                                        </FormField>
                                                        <FormField label="Target column" name="csv_target_col">
                                                            <Select
                                                                value=csv_target_col
                                                                options=target_options
                                                                on_change=on_csv_target_change
                                                            />
                                                        </FormField>
                                                        <FormField label="Weight column (optional)" name="csv_weight_col">
                                                            <Select
                                                                value=csv_weight_col
                                                                options=weight_options
                                                                on_change=on_csv_weight_change
                                                            />
                                                        </FormField>
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                </Card>
                            }.into_any(),
                            UploadMode::Text => view! {
                                <Card>
                                    <div class="p-4 space-y-3">
                                        <div class="flex items-center justify-between">
                                            <div>
                                                <div class="text-sm font-medium">"Text / Markdown"</div>
                                                <p class="text-xs text-muted-foreground">
                                                    "Choose how to pair blocks: echo uses the same text for input and target; pairing consumes adjacent blocks."
                                                </p>
                                            </div>
                                            <Badge variant=BadgeVariant::Secondary>"Input / Target"</Badge>
                                        </div>
                                        <input
                                            type="file"
                                            accept=".txt,.md,.markdown"
                                            class="mt-1 block w-full text-sm"
                                            on:change=move |ev| data_handler.run(ev)
                                        />
                                        <div class="flex gap-2">
                                            {vec![
                                                (TextStrategy::Echo, "Echo input as target"),
                                                (TextStrategy::PairAdjacent, "Pair adjacent blocks"),
                                            ].into_iter().map(|(value, label)| {
                                                view! {
                                                    <button
                                                        class=move || {
                                                            if text_strategy.try_get().unwrap_or(TextStrategy::Echo) == value {
                                                                "px-3 py-2 rounded-md bg-primary text-primary-foreground text-xs"
                                                            } else {
                                                                "px-3 py-2 rounded-md border text-xs"
                                                            }
                                                        }
                                                        on:click=move |_| {
                                                            text_strategy.set(value);
                                                            refresh_preview.run(());
                                                        }
                                                    >
                                                        {label}
                                                    </button>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </div>
                                </Card>
                            }.into_any(),
                        }}

                        // Parse errors (file-level)
                        {move || {
                            let errors = parse_errors.try_get().unwrap_or_default();
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

                        // Form validation errors
                        {move || {
                            let fe = form_errors.try_get().unwrap_or_default();
                            if !fe.has_any() {
                                view! {}.into_any()
                            } else {
                                let items: Vec<String> = fe.all().values().cloned().collect();
                                view! {
                                    <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive space-y-1">
                                        {items.into_iter().map(|e| view! { <div>{e}</div> }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }}

                        {move || {
                            let rows = preview_rows.try_get().unwrap_or_default();
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

                        // Upload error (inline banner)
                        {move || upload_error.try_get().flatten().map(|err| view! {
                            <div class="rounded-md border border-destructive bg-destructive/10 p-3 text-sm text-destructive">
                                {err}
                            </div>
                        })}
                        {move || {
                            let msg = status.try_get().unwrap_or_default();
                            if msg.is_empty() {
                                view! {}.into_any()
                            } else {
                                view! {
                                    <div class="rounded-md border border-status-success/50 bg-status-success/10 p-3 text-sm text-foreground">
                                        {msg}
                                    </div>
                                }.into_any()
                            }
                        }}

                        <div class="flex justify-end gap-2">
                            <Button variant=ButtonVariant::Ghost on_click=Callback::new(move |_| open.set(false))>"Cancel"</Button>
                            <Button
                                variant=ButtonVariant::Primary
                                disabled=submitting
                                on_click=on_upload
                            >
                                <Show
                                    when=move || submitting.try_get().unwrap_or(false)
                                    fallback=move || view! { "Upload dataset" }
                                >
                                    <div class="flex items-center gap-2"><Spinner size=SpinnerSize::Sm/> "Uploading..."</div>
                                </Show>
                            </Button>
                        </div>
                    </div>
                    </div>
                </Dialog>
            }.into_any()
        }
    };

    view! {
        {dialog}
    }
}
