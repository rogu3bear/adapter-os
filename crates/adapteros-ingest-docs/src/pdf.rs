use crate::chunker::DocumentChunker;
use crate::types::{
    DocumentSource, ExtractionConfidence, IngestedDocument, IngestedDocumentWithErrors,
    PageExtractionResult,
};
use crate::utils::{finalize_chunks, normalize_whitespace};
use adapteros_core::{AosError, B3Hash, Result};
use lopdf::{Document, Object};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Resource guardrails for untrusted PDFs. These limits are intentionally strict to
// avoid excessive memory usage or pathological page tree recursion from crafted files.
const MAX_PDF_BYTES: usize = 25 * 1024 * 1024; // 25 MiB input ceiling
const MAX_PDF_OBJECTS: usize = 20_000; // catch object explosion before traversal
const MAX_PDF_PAGES: usize = 2_000; // avoid unbounded page walks
const MAX_PAGE_TREE_DEPTH: usize = 64; // tighten below lopdf's internal 256 guard
const MAX_PAGE_TEXT_CHARS: usize = 1_000_000; // per-page text upper bound after normalization

pub fn ingest_pdf_path(path: &Path, chunker: &DocumentChunker) -> Result<IngestedDocument> {
    let bytes = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read PDF file {}: {e}", path.display())))?;
    let name = crate::source_name_from_path(path);
    ingest_pdf_bytes(&bytes, &name, Some(path.to_path_buf()), chunker)
}

pub fn ingest_pdf_bytes(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocument> {
    let doc_hash = B3Hash::hash(bytes);
    let mut document = load_pdf_with_limits(bytes, source_name)?;

    if document.is_encrypted() {
        document
            .decrypt("")
            .map_err(|_| AosError::Validation("Encrypted PDFs are not supported".to_string()))?;
    }

    let pages = pages_with_limits(&document, source_name)?;

    let mut all_chunks = Vec::new();
    let mut normalized_text = String::new();
    for (page_number, _object_id) in pages.iter() {
        let text = document.extract_text(&[*page_number]).map_err(|e| {
            AosError::Validation(format!(
                "Failed to extract text from page {} of {}: {e}",
                page_number, source_name
            ))
        })?;
        let normalized = normalize_whitespace(&text);
        if normalized.trim().is_empty() {
            continue;
        }
        if normalized.len() > MAX_PAGE_TEXT_CHARS {
            return Err(AosError::Validation(format!(
                "PDF {} page {} text exceeds limit ({} chars > {})",
                source_name,
                page_number,
                normalized.len(),
                MAX_PAGE_TEXT_CHARS
            )));
        }
        if !normalized_text.is_empty() {
            normalized_text.push_str("\n\n");
        }
        normalized_text.push_str(&normalized);
        let mut page_chunks = chunker.chunk(&normalized, Some(*page_number))?;
        all_chunks.append(&mut page_chunks);
    }

    if normalized_text.trim().is_empty() {
        return Err(AosError::Validation(format!(
            "PDF {} contains no text layer",
            source_name
        )));
    }

    let chunks = finalize_chunks(all_chunks);
    let normalized_text_hash = B3Hash::hash(normalized_text.as_bytes());
    let normalized_text_len = normalized_text.chars().count();

    Ok(IngestedDocument {
        source: DocumentSource::Pdf,
        source_name: source_name.to_string(),
        source_path,
        doc_hash,
        normalized_text_hash: Some(normalized_text_hash),
        normalized_text_len: Some(normalized_text_len),
        byte_len: bytes.len(),
        page_count: Some(pages.len()),
        chunks,
    })
}

/// Ingest PDF with per-page error recovery.
/// Unlike `ingest_pdf_bytes`, this continues processing even if some pages fail.
pub fn ingest_pdf_bytes_resilient(
    bytes: &[u8],
    source_name: &str,
    source_path: Option<PathBuf>,
    chunker: &DocumentChunker,
) -> Result<IngestedDocumentWithErrors> {
    let doc_hash = B3Hash::hash(bytes);
    let mut document = load_pdf_with_limits(bytes, source_name)?;

    // Handle encryption
    if document.is_encrypted() {
        document.decrypt("").map_err(|_| {
            AosError::Validation(format!("Encrypted PDF {} requires password", source_name))
        })?;
    }

    let pages = pages_with_limits(&document, source_name)?;

    let total_pages = pages.len();
    let mut all_chunks = Vec::new();
    let mut page_errors = Vec::new();
    let mut successful_pages = 0;
    let mut pages_with_images = 0;
    let pages_with_visual_extraction = 0;
    let mut normalized_text = String::new();

    for (page_number, object_id) in pages.iter() {
        // Check if page contains images that won't be extracted as text
        let has_images = page_has_images(&document, *object_id);
        if has_images {
            pages_with_images += 1;
        }

        let (visual_content_extracted, visual_description) = (false, None);

        match document.extract_text(&[*page_number]) {
            Ok(text) => {
                let normalized = normalize_whitespace(&text);

                if normalized.trim().is_empty() && visual_description.is_none() {
                    // Empty page with no visual content - skip
                    page_errors.push(PageExtractionResult {
                        page_number: *page_number,
                        text: None,
                        error: Some("Empty page".to_string()),
                        has_unextracted_images: has_images && !visual_content_extracted,
                        visual_content_extracted,
                        visual_description,
                    });
                    continue;
                }

                if normalized.len() > MAX_PAGE_TEXT_CHARS {
                    tracing::warn!(
                        page = page_number,
                        source = source_name,
                        length = normalized.len(),
                        limit = MAX_PAGE_TEXT_CHARS,
                        "Page text exceeds limit, skipping"
                    );
                    page_errors.push(PageExtractionResult {
                        page_number: *page_number,
                        text: None,
                        error: Some(format!(
                            "Page text too large ({} chars > {})",
                            normalized.len(),
                            MAX_PAGE_TEXT_CHARS
                        )),
                        has_unextracted_images: has_images && !visual_content_extracted,
                        visual_content_extracted,
                        visual_description,
                    });
                    continue;
                }

                if !normalized.trim().is_empty() {
                    if !normalized_text.is_empty() {
                        normalized_text.push_str("\n\n");
                    }
                    normalized_text.push_str(&normalized);
                }

                match chunker.chunk(&normalized, Some(*page_number)) {
                    Ok(mut page_chunks) => {
                        all_chunks.append(&mut page_chunks);
                        successful_pages += 1;
                        page_errors.push(PageExtractionResult {
                            page_number: *page_number,
                            text: Some(normalized),
                            error: None,
                            has_unextracted_images: has_images && !visual_content_extracted,
                            visual_content_extracted,
                            visual_description,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            page = page_number,
                            error = %e,
                            source = source_name,
                            "Failed to chunk page content, skipping"
                        );
                        page_errors.push(PageExtractionResult {
                            page_number: *page_number,
                            text: Some(normalized),
                            error: Some(format!("Chunking failed: {}", e)),
                            has_unextracted_images: has_images && !visual_content_extracted,
                            visual_content_extracted,
                            visual_description,
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    page = page_number,
                    error = %e,
                    source = source_name,
                    "Failed to extract text from page, skipping"
                );
                page_errors.push(PageExtractionResult {
                    page_number: *page_number,
                    text: None,
                    error: Some(format!("Text extraction failed: {}", e)),
                    has_unextracted_images: has_images && !visual_content_extracted,
                    visual_content_extracted,
                    visual_description,
                });
            }
        }
    }

    if normalized_text.trim().is_empty() {
        return Err(AosError::Validation(format!(
            "PDF {} contains no text layer",
            source_name
        )));
    }

    // Log summary
    if successful_pages < total_pages {
        tracing::info!(
            source = source_name,
            total_pages = total_pages,
            successful_pages = successful_pages,
            failed_pages = total_pages - successful_pages,
            "PDF partially processed with some page failures"
        );
    }

    // Log summary of visual content extraction
    if pages_with_images > 0 {
        if pages_with_visual_extraction > 0 {
            tracing::info!(
                source = source_name,
                pages_with_images = pages_with_images,
                pages_with_visual_extraction = pages_with_visual_extraction,
                total_pages = total_pages,
                "PDF visual content extracted from pages"
            );
        } else {
            tracing::info!(
                source = source_name,
                pages_with_images = pages_with_images,
                total_pages = total_pages,
                "PDF contains pages with images/charts that could not be extracted"
            );
        }
    }

    let chunks = finalize_chunks(all_chunks);
    let normalized_text_hash = B3Hash::hash(normalized_text.as_bytes());
    let normalized_text_len = normalized_text.chars().count();

    // Compute extraction confidence from results
    let extraction_confidence = ExtractionConfidence::compute(
        total_pages,
        successful_pages,
        pages_with_images,
        &page_errors,
    );

    // Log confidence if degraded
    if !extraction_confidence.is_acceptable() {
        tracing::warn!(
            source = source_name,
            text_score = extraction_confidence.text_score,
            degraded_pages = ?extraction_confidence.degraded_pages,
            reason = ?extraction_confidence.degradation_reason,
            "Document extraction confidence below threshold"
        );
    }

    Ok(IngestedDocumentWithErrors {
        document: IngestedDocument {
            source: DocumentSource::Pdf,
            source_name: source_name.to_string(),
            source_path,
            doc_hash,
            normalized_text_hash: Some(normalized_text_hash),
            normalized_text_len: Some(normalized_text_len),
            byte_len: bytes.len(),
            page_count: Some(total_pages),
            chunks,
        },
        page_errors,
        total_pages,
        successful_pages,
        pages_with_images,
        pages_with_visual_extraction,
        extraction_confidence,
    })
}

fn load_pdf_with_limits(bytes: &[u8], source_name: &str) -> Result<Document> {
    if bytes.is_empty() {
        return Err(AosError::Validation(format!("PDF {source_name} is empty")));
    }

    if bytes.len() > MAX_PDF_BYTES {
        return Err(AosError::Validation(format!(
            "PDF {source_name} is too large ({} bytes > {} byte limit)",
            bytes.len(),
            MAX_PDF_BYTES
        )));
    }

    let document = Document::load_mem(bytes)
        .map_err(|e| AosError::Validation(format!("Failed to parse PDF {source_name}: {e}")))?;

    if document.objects.len() > MAX_PDF_OBJECTS {
        return Err(AosError::Validation(format!(
            "PDF {source_name} contains too many objects ({} > {})",
            document.objects.len(),
            MAX_PDF_OBJECTS
        )));
    }

    Ok(document)
}

fn pages_with_limits(
    document: &Document,
    source_name: &str,
) -> Result<std::collections::BTreeMap<u32, lopdf::ObjectId>> {
    validate_page_tree(document, source_name)?;

    let pages = document.get_pages();
    if pages.is_empty() {
        return Err(AosError::Validation(format!(
            "PDF {source_name} contains no pages"
        )));
    }

    if pages.len() > MAX_PDF_PAGES {
        return Err(AosError::Validation(format!(
            "PDF {source_name} has {} pages which exceeds the limit of {}",
            pages.len(),
            MAX_PDF_PAGES
        )));
    }

    let mut seen_pages = HashSet::new();
    for object_id in pages.values() {
        if !seen_pages.insert(*object_id) {
            return Err(AosError::Validation(format!(
                "PDF {source_name} page tree contains duplicate or cyclic references"
            )));
        }
    }

    Ok(pages)
}

/// Check if a PDF page contains image XObjects that won't be extracted as text.
/// Returns true if the page has embedded images (charts, figures, scanned content).
fn page_has_images(document: &Document, page_id: lopdf::ObjectId) -> bool {
    let Ok(page_dict) = document.get_dictionary(page_id) else {
        return false;
    };

    // Check Resources -> XObject dictionary for Image subtypes
    let resources = if let Ok(res_ref) = page_dict.get(b"Resources") {
        match res_ref {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => document.get_dictionary(*r).ok().cloned(),
            _ => None,
        }
    } else {
        None
    };

    let Some(resources) = resources else {
        return false;
    };

    let xobjects = if let Ok(xobj_ref) = resources.get(b"XObject") {
        match xobj_ref {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => document.get_dictionary(*r).ok().cloned(),
            _ => None,
        }
    } else {
        None
    };

    let Some(xobjects) = xobjects else {
        return false;
    };

    for (_name, obj_ref) in xobjects.iter() {
        if let Ok(obj_id) = obj_ref.as_reference() {
            if let Ok(xobj_dict) = document.get_dictionary(obj_id) {
                if let Ok(subtype) = xobj_dict.get(b"Subtype") {
                    if let Ok(name) = subtype.as_name() {
                        if name == b"Image" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn validate_page_tree(document: &Document, source_name: &str) -> Result<()> {
    // Walk the Pages tree to enforce a maximum depth and catch cycles before extracting pages.
    let catalog = document
        .catalog()
        .map_err(|e| AosError::Validation(format!("Invalid PDF catalog in {source_name}: {e}")))?;
    let Some(pages_ref) = catalog.get(b"Pages").and_then(Object::as_reference).ok() else {
        return Err(AosError::Validation(format!(
            "PDF {source_name} is missing a Pages root"
        )));
    };

    let mut stack = vec![(pages_ref, 0usize)];
    let mut seen_nodes = HashSet::new();

    while let Some((node_id, depth)) = stack.pop() {
        if depth > MAX_PAGE_TREE_DEPTH {
            return Err(AosError::Validation(format!(
                "PDF {source_name} page tree depth exceeded limit of {}",
                MAX_PAGE_TREE_DEPTH
            )));
        }

        if !seen_nodes.insert(node_id) {
            return Err(AosError::Validation(format!(
                "PDF {source_name} page tree contains recursion"
            )));
        }

        let dict = document.get_dictionary(node_id).map_err(|e| {
            AosError::Validation(format!("Invalid page tree in {source_name}: {e}"))
        })?;

        if let Ok(count) = dict.get(b"Count").and_then(Object::as_i64) {
            if count > MAX_PDF_PAGES as i64 {
                return Err(AosError::Validation(format!(
                    "PDF {source_name} declares {} pages which exceeds the limit of {}",
                    count, MAX_PDF_PAGES
                )));
            }
        }

        if let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array) {
            for kid in kids {
                if let Ok(kid_id) = kid.as_reference() {
                    if let Ok(kid_dict) = document.get_dictionary(kid_id) {
                        if let Ok(type_name) = kid_dict.get_type() {
                            if type_name == b"Pages" {
                                stack.push((kid_id, depth + 1));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
