use adapteros_config::prepare_socket_path;
use tempfile::TempDir;

#[tokio::test]
async fn stale_socket_is_removed_and_binding_succeeds() {
    let tmp = TempDir::new_in(".").expect("tmp dir");
    let socket_path = tmp.path().join("worker.sock");
    std::fs::create_dir_all(socket_path.parent().unwrap()).expect("mkdirs");
    std::fs::write(&socket_path, b"stale").expect("write stale socket");

    prepare_socket_path(&socket_path, "worker").expect("prepare socket path");

    let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind after cleanup");
    assert!(socket_path.exists());
    drop(listener);
}

#[tokio::test]
async fn tmp_socket_path_is_rejected() {
    let socket_path = std::path::PathBuf::from("/tmp/aos-worker.sock");
    let err = prepare_socket_path(&socket_path, "worker").expect_err("reject /tmp");
    assert!(
        err.to_string().contains("/tmp"),
        "error should mention /tmp"
    );
}

#[tokio::test]
async fn private_tmp_socket_path_is_rejected() {
    let socket_path = std::path::PathBuf::from("/private/tmp/aos-worker.sock");
    let err = prepare_socket_path(&socket_path, "worker").expect_err("reject /private/tmp");
    assert!(
        err.to_string().contains("/tmp"),
        "error should mention tmp prefix"
    );
}
