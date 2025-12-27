use rusqlite::Connection;
use std::process::Command;
use tempfile::TempDir;

fn create_dirty_db(temp_dir: &TempDir) -> std::path::PathBuf {
    let db_path = temp_dir.path().join("dirty.db");
    let conn = Connection::open(&db_path).expect("create temp db");

    conn.execute(
        "CREATE TABLE _sqlx_migrations (version TEXT, description TEXT, success INTEGER, checksum TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum) VALUES ('20240101', 'test migration', 0, 'deadbeef')",
        [],
    )
    .unwrap();
    drop(conn);

    let file_name = db_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let wal_path = db_path.with_file_name(format!("{}-wal", file_name));
    let shm_path = db_path.with_file_name(format!("{}-shm", file_name));
    std::fs::write(&wal_path, b"wal").unwrap();
    std::fs::write(&shm_path, b"shm").unwrap();

    db_path
}

#[test]
fn db_unlock_clears_dirty_state_and_wal_files() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = create_dirty_db(&temp_dir);

    let status = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aosctl",
            "--",
            "db",
            "unlock",
            "--db-path",
            db_path.to_string_lossy().as_ref(),
        ])
        .env("AOS_SKIP_MIGRATION_SIGNATURES", "1")
        .status()
        .expect("failed to run aosctl db unlock");
    assert!(status.success(), "aosctl db unlock failed: {:?}", status);

    let conn = Connection::open(&db_path).expect("open temp db");
    let remaining: i64 = conn
        .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
            row.get(0)
        })
        .expect("count rows");
    assert_eq!(remaining, 0, "dirty migration rows should be cleared");

    let file_name = db_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let wal_path = db_path.with_file_name(format!("{}-wal", file_name));
    let shm_path = db_path.with_file_name(format!("{}-shm", file_name));
    assert!(
        !wal_path.exists() && !shm_path.exists(),
        "wal/shm files should be removed"
    );
}
