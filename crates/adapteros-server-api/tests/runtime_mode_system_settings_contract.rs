use adapteros_db::Db;
use adapteros_server_api::config::Config;
use adapteros_server_api::runtime_mode::{RuntimeMode, RuntimeModeResolver};

mod common;

fn test_config(production_mode: bool, uds_socket: Option<&str>, require_pf_deny: bool) -> Config {
    let uds_socket_line = uds_socket
        .map(|value| format!("uds_socket = \"{}\"", value))
        .unwrap_or_default();

    let contents = format!(
        r#"
[server]
port = 8080
production_mode = {}
{}

[db]
path = "var/aos-cp.sqlite3"

[security]
jwt_secret = "valid_secret_that_is_long_enough_for_testing"
require_pf_deny = {}

[paths]
artifacts_root = "var/artifacts"
bundles_root = "var/bundles"

[rate_limits]
requests_per_minute = 100
burst_size = 50
inference_per_minute = 100

[metrics]
enabled = false
bearer_token = "token"
include_histogram = false
histogram_buckets = [0.1, 0.5, 1.0]

[alerting]
enabled = false
alert_dir = "var/alerts"
max_alerts_per_file = 10
rotate_size_mb = 5
"#,
        production_mode, uds_socket_line, require_pf_deny
    );

    toml::from_str(&contents).expect("test config should parse")
}

#[tokio::test]
async fn resolve_runtime_mode_from_system_settings() {
    let _guard = common::env_lock().await;
    std::env::remove_var("AOS_RUNTIME_MODE");

    let db = Db::new_in_memory()
        .await
        .expect("in-memory DB should initialize");
    db.set_system_setting("runtime_mode", "staging")
        .await
        .expect("runtime_mode should be writable");

    let config = test_config(false, None, true);
    let mode = RuntimeModeResolver::resolve(&config, &db)
        .await
        .expect("runtime mode should resolve");

    assert_eq!(mode, RuntimeMode::Staging);
}

#[tokio::test]
async fn validate_prod_jwt_mode_from_system_settings() {
    let db = Db::new_in_memory()
        .await
        .expect("in-memory DB should initialize");
    let config = test_config(true, Some("/tmp/aos-runtime-mode.sock"), true);

    db.set_system_setting("jwt_mode", "eddsa")
        .await
        .expect("jwt_mode should be writable");
    RuntimeModeResolver::validate(RuntimeMode::Prod, &config, &db)
        .await
        .expect("eddsa jwt_mode should pass production validation");

    db.set_system_setting("jwt_mode", "hmac")
        .await
        .expect("jwt_mode should update");
    let error = RuntimeModeResolver::validate(RuntimeMode::Prod, &config, &db)
        .await
        .expect_err("non-eddsa jwt_mode should fail production validation");

    assert!(
        error.contains("system_settings.jwt_mode"),
        "error should mention system_settings key"
    );
}
