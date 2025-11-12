#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use crate::state::AppState;
    use sqlx::SqlitePool;

    #[tokio::test]
    async fn test_replay_from_bundle() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query("CREATE TABLE telemetry_bundles (id TEXT PRIMARY KEY, cpid TEXT, plan_id TEXT, metadata_json TEXT);")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO telemetry_bundles (id, cpid, plan_id) VALUES ('test-bundle', 'test-cpid', 'test-plan');")
            .execute(&pool)
            .await
            .unwrap();

        let state = AppState::new_with_db(sqlx::Pool::from(pool)); // Assume new_with_db for test
        let claims = Claims { tenant_id: "test".to_string(), ... Default }; // Stub
        let response = replay_from_bundle(State(state), Extension(claims), Path("test-bundle".to_string())).await.unwrap();
        let body = response.unwrap().into_parts().1; // Parse JSON
        let session: ReplaySessionResponse = serde_json::from_str(&body); // Assert cpid == "test-cpid"
        assert_eq!(session.cpid, "test-cpid");
    }
}
