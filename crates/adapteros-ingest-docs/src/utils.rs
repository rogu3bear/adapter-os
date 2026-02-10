use crate::types::{ChunkProvenance, DocumentChunk};

pub fn normalize_whitespace(input: &str) -> String {
    let content = input.replace('\r', "");
    let mut result = String::new();
    let mut last_blank = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !last_blank {
                result.push('\n');
                last_blank = true;
            }
            continue;
        }

        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(trimmed);
        last_blank = false;
    }

    if result.is_empty() {
        content.trim().to_string()
    } else {
        result
    }
}

pub fn finalize_chunks(
    mut chunks: Vec<DocumentChunk>,
    provenance: &ChunkProvenance,
) -> Vec<DocumentChunk> {
    let total = chunks.len();
    for chunk in &mut chunks {
        chunk.total_chunks = total;
        chunk.provenance = Some(provenance.clone());
    }
    chunks
}
