use std::fs;
use std::path::Path;
use adapteros_server::config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for testing
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path();
    
    // Create test directories
    fs::create_dir_all(temp_path.join("adapters"))?;
    fs::create_dir_all(temp_path.join("artifacts"))?;
    fs::create_dir_all(temp_path.join("bundles"))?;
    fs::create_dir_all(temp_path.join("plan"))?;
    fs::create_dir_all(temp_path.join("alerts"))?;
    
    // Create a test config
    let config_content = format!(r#"
[server]
port = 8080
bind = "127.0.0.1"

[db]
path = "{}/test.db"

[security]
jwt_secret = "test_secret_32_chars_long_enough"
global_seed = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[paths]
adapters_root = "{}/adapters"
artifacts_root = "{}/artifacts"
bundles_root = "{}/bundles"
plan_dir = "{}/plan"

[alerting]
enabled = true
alert_dir = "{}/alerts"
max_alerts_per_file = 1000
rotate_size_mb = 10
"#, temp_path.display(), temp_path.display(), temp_path.display(), temp_path.display(), temp_path.display());
    
    let config_path = temp_path.join("test_config.toml");
    fs::write(&config_path, config_content)?;
    
    // Test loading and validation
    let mut config = Config::load(config_path.to_str().unwrap())?;
    println!("Config loaded successfully");
    
    config.validate()?;
    println!("Config validation passed");
    
    println!("✅ Config validation test passed!");
    Ok(())
}
