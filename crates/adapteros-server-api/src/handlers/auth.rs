use axum::{http::StatusCode, response::Json, Json as AxumJson};
use adapteros_db::Db;
use tracing::warn;

pub async fn login_handler(
    AxumJson(payload): AxumJson<LoginRequest>,
    db: Db,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Check if users table is empty
    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if user_count == 0 {
        warn!("No users seeded in DB; bootstrap required");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Existing login logic...
    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE username = $1", payload.username)
        .fetch_optional(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ... rest of validation and JWT generation ...
    Ok(Json(response))
}

