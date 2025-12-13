use super::fs_utils::ensure_dirs;
use super::paths::DatasetPaths;
use crate::error_helpers::{internal_error, not_found, payload_too_large};
use crate::handlers::chunked_upload::{
    ChunkAssembler, ChunkWriter, CompressionFormat, ResumeToken, UploadSession,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::Json;
use tracing::{error, warn};

pub fn expected_chunks(total_size: u64, chunk_size: usize) -> usize {
    ((total_size + (chunk_size as u64 - 1)) / (chunk_size as u64)) as usize
}

pub async fn prepare_session(
    state: &AppState,
    paths: &DatasetPaths,
    file_name: &str,
    total_size: u64,
    content_type: &str,
    chunk_size: usize,
    compression: CompressionFormat,
) -> Result<(UploadSession, usize), (StatusCode, Json<ErrorResponse>)> {
    ensure_dirs([paths.chunked.as_path()]).await?;

    let session = state
        .upload_session_manager
        .create_session(
            file_name.to_string(),
            total_size,
            content_type.to_string(),
            chunk_size,
            &paths.chunked,
        )
        .await
        .map_err(internal_error)?;

    Ok((session, expected_chunks(total_size, chunk_size)))
}

pub async fn persist_chunk(
    state: &AppState,
    session_id: &str,
    chunk_index: usize,
    body: &Bytes,
) -> Result<
    (UploadSession, usize, String, usize, bool, Option<String>),
    (StatusCode, Json<ErrorResponse>),
> {
    let session = state
        .upload_session_manager
        .get_session(session_id)
        .await
        .map_err(|_| not_found("Upload session"))?;

    let expected_chunks = expected_chunks(session.total_size, session.chunk_size);
    if chunk_index >= expected_chunks {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!(
                    "Invalid chunk index {}. Expected 0-{} for {} total chunks",
                    chunk_index,
                    expected_chunks - 1,
                    expected_chunks
                ),
                code: "INVALID_CHUNK_INDEX".to_string(),
                failure_code: None,
                details: None,
            }),
        ));
    }

    if session.received_chunks.contains_key(&chunk_index) {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                error: format!("Chunk {} has already been uploaded", chunk_index),
                code: "DUPLICATE_CHUNK".to_string(),
                failure_code: None,
                details: None,
            }),
        ));
    }

    let is_last_chunk = chunk_index == expected_chunks - 1;
    let expected_chunk_size = if is_last_chunk {
        let remainder = session.total_size % (session.chunk_size as u64);
        if remainder == 0 {
            session.chunk_size
        } else {
            remainder as usize
        }
    } else {
        session.chunk_size
    };

    if body.len() > session.chunk_size {
        return Err(payload_too_large(&format!(
            "Chunk size {} exceeds maximum chunk size {}",
            body.len(),
            session.chunk_size
        )));
    }

    if body.len() < expected_chunk_size && !is_last_chunk {
        warn!(
            "Chunk {} for session {} smaller than expected ({} < {})",
            chunk_index,
            session_id,
            body.len(),
            expected_chunk_size
        );
    }

    let chunk_path = session.temp_dir.join(format!("chunk_{:08}", chunk_index));
    let mut writer = ChunkWriter::new(&chunk_path).await.map_err(|e| {
        error!("Failed to create chunk writer: {}", e);
        internal_error(format!("Failed to create chunk file: {}", e))
    })?;

    writer.write_chunk(body).await.map_err(|e| {
        error!("Failed to write chunk data: {}", e);
        internal_error(format!("Failed to write chunk: {}", e))
    })?;

    let chunk_hash = writer.finalize().await.map_err(|e| {
        error!("Failed to finalize chunk: {}", e);
        internal_error(format!("Failed to finalize chunk: {}", e))
    })?;

    state
        .upload_session_manager
        .add_chunk(session_id, chunk_index, chunk_hash.clone())
        .await
        .map_err(|e| {
            error!("Failed to update session: {}", e);
            internal_error(format!("Failed to update session: {}", e))
        })?;

    let updated_session = state
        .upload_session_manager
        .get_session(session_id)
        .await
        .map_err(internal_error)?;

    let chunks_received = updated_session.received_chunks.len();
    let is_complete = state
        .upload_session_manager
        .is_upload_complete(session_id)
        .await
        .unwrap_or(false);

    let resume_token = if !is_complete {
        let next_chunk = (0..expected_chunks)
            .find(|i| !updated_session.received_chunks.contains_key(i))
            .unwrap_or(expected_chunks);

        Some(
            serde_json::to_string(&ResumeToken {
                session_id: session_id.to_string(),
                next_chunk,
                hash_state: chunk_hash.clone(),
            })
            .unwrap_or_default(),
        )
    } else {
        None
    };

    Ok((
        updated_session,
        expected_chunks,
        chunk_hash,
        chunks_received,
        is_complete,
        resume_token,
    ))
}

pub async fn assemble_chunks(
    session: &UploadSession,
    output_path: &std::path::Path,
) -> Result<(String, u64), (StatusCode, Json<ErrorResponse>)> {
    let assembler = ChunkAssembler::new(
        output_path.to_path_buf(),
        session.temp_dir.clone(),
        session.chunk_size,
        session.total_size,
        session.compression.clone(),
    );

    assembler
        .assemble()
        .await
        .map_err(|e| internal_error(format!("Failed to assemble file: {}", e)))
}
