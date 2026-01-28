use super::fs_utils::ensure_dirs;
use super::paths::DatasetPaths;
use crate::api_error::ApiError;
use crate::handlers::chunked_upload::{
    ChunkAssembler, ChunkWriter, CompressionFormat, ResumeToken, UploadSession,
    UploadSessionManager,
};
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;

pub fn expected_chunks(total_size: u64, chunk_size: usize) -> usize {
    total_size.div_ceil(chunk_size as u64) as usize
}

pub fn expected_chunk_size(total_size: u64, chunk_size: usize, chunk_index: usize) -> usize {
    let total_chunks = expected_chunks(total_size, chunk_size);
    let is_last_chunk = chunk_index + 1 == total_chunks;
    if is_last_chunk {
        let remainder = total_size % (chunk_size as u64);
        if remainder == 0 {
            chunk_size
        } else {
            remainder as usize
        }
    } else {
        chunk_size
    }
}

#[allow(dead_code)]
pub async fn prepare_session(
    state: &AppState,
    paths: &DatasetPaths,
    file_name: &str,
    total_size: u64,
    content_type: &str,
    chunk_size: usize,
    compression: CompressionFormat,
) -> Result<(UploadSession, usize), (StatusCode, Json<ErrorResponse>)> {
    prepare_session_with_workspace(
        state,
        paths,
        file_name,
        total_size,
        content_type,
        chunk_size,
        compression,
        None,
    )
    .await
}

/// Prepare a session with optional workspace ID for tenant isolation
#[allow(clippy::too_many_arguments)]
pub async fn prepare_session_with_workspace(
    state: &AppState,
    paths: &DatasetPaths,
    file_name: &str,
    total_size: u64,
    content_type: &str,
    chunk_size: usize,
    _compression: CompressionFormat,
    workspace_id: Option<String>,
) -> Result<(UploadSession, usize), (StatusCode, Json<ErrorResponse>)> {
    ensure_dirs([paths.chunked.as_path()]).await?;

    let session = state
        .upload_session_manager
        .create_session_with_workspace(
            file_name.to_string(),
            total_size,
            content_type.to_string(),
            chunk_size,
            &paths.chunked,
            workspace_id,
        )
        .await
        .map_err(|e| {
            let err: (StatusCode, Json<ErrorResponse>) = ApiError::internal(e.to_string()).into();
            err
        })?;

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
        .map_err(|_| {
            let err: (StatusCode, Json<ErrorResponse>) = ApiError::not_found("Upload session").into();
            err
        })?;
    if UploadSessionManager::is_session_expired(&session) {
        let err: (StatusCode, Json<ErrorResponse>) = ApiError::not_found("Upload session").into();
        return Err(err);
    }

    let expected_chunks = expected_chunks(session.total_size, session.chunk_size);
    if chunk_index >= expected_chunks {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                message: format!(
                    "Invalid chunk index {}. Expected 0-{} for {} total chunks",
                    chunk_index,
                    expected_chunks - 1,
                    expected_chunks
                ),
                code: "INVALID_CHUNK_INDEX".to_string(),
                failure_code: None,
                details: None,
                hint: None,
                request_id: None,
            }),
        ));
    }

    if let Some(existing_hash) = session.received_chunks.get(&chunk_index) {
        let incoming_hash = blake3::hash(body).to_hex().to_string();
        if &incoming_hash == existing_hash {
            let chunks_received = session.received_chunks.len();
            let is_complete = state
                .upload_session_manager
                .is_upload_complete(session_id)
                .await
                .unwrap_or(false);

            let resume_token = if !is_complete {
                let next_chunk = (0..expected_chunks)
                    .find(|i| !session.received_chunks.contains_key(i))
                    .unwrap_or(expected_chunks);

                Some(
                    serde_json::to_string(&ResumeToken {
                        session_id: session_id.to_string(),
                        next_chunk,
                        hash_state: incoming_hash.clone(),
                    })
                    .unwrap_or_default(),
                )
            } else {
                None
            };

            return Ok((
                session,
                expected_chunks,
                incoming_hash,
                chunks_received,
                is_complete,
                resume_token,
            ));
        }

        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                message: format!(
                    "Chunk {} has already been uploaded with a different hash",
                    chunk_index
                ),
                code: "CHUNK_HASH_MISMATCH".to_string(),
                failure_code: None,
                details: None,
                hint: None,
                request_id: None,
            }),
        ));
    }

    let expected_chunk_size =
        expected_chunk_size(session.total_size, session.chunk_size, chunk_index);
    if body.len() != expected_chunk_size {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                message: format!(
                    "Invalid chunk size {}. Expected {} bytes for chunk {}",
                    body.len(),
                    expected_chunk_size,
                    chunk_index
                ),
                code: "INVALID_CHUNK_SIZE".to_string(),
                failure_code: None,
                details: None,
                hint: None,
                request_id: None,
            }),
        ));
    }

    let chunk_path = session.temp_dir.join(format!("chunk_{:08}", chunk_index));
    let mut writer = ChunkWriter::new(&chunk_path).await.map_err(|e| {
        error!("Failed to create chunk writer: {}", e);
        ApiError::internal(format!("Failed to create chunk file: {}", e))
    })?;

    writer.write_chunk(body).await.map_err(|e| {
        error!("Failed to write chunk data: {}", e);
        ApiError::internal(format!("Failed to write chunk: {}", e))
    })?;

    let chunk_hash = writer.finalize().await.map_err(|e| {
        error!("Failed to finalize chunk: {}", e);
        ApiError::internal(format!("Failed to finalize chunk: {}", e))
    })?;

    state
        .upload_session_manager
        .add_chunk(session_id, chunk_index, chunk_hash.clone())
        .await
        .map_err(|e| {
            error!("Failed to update session: {}", e);
            ApiError::internal(format!("Failed to update session: {}", e))
        })?;

    let updated_session = state
        .upload_session_manager
        .get_session(session_id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

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
        .map_err(|e| ApiError::internal(format!("Failed to assemble file: {}", e)).into())
}
