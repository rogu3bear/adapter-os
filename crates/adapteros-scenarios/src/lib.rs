use adapteros_core::{AosError, Result};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const ENV_SCENARIO_DIR: &str = "AOS_SCENARIOS_DIR";
pub const DEFAULT_SCENARIO_DIR: &str = "configs/scenarios";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub struct ScenarioId(String);

impl ScenarioId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ScenarioId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for ScenarioId {
    fn from(value: &str) -> Self {
        ScenarioId::new(value)
    }
}

impl From<String> for ScenarioId {
    fn from(value: String) -> Self {
        ScenarioId::new(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioConfig {
    pub id: ScenarioId,
    #[serde(default)]
    pub description: Option<String>,
    pub tenant: TenantConfig,
    pub model: ModelConfig,
    pub adapter: AdapterConfig,
    #[serde(default)]
    pub training: Option<TrainingConfig>,
    #[serde(default)]
    pub chat: Option<ChatConfig>,
    #[serde(default)]
    pub replay: Option<ReplayConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenantConfig {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    #[serde(default)]
    pub warmup: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_model_id: Option<String>,
    #[serde(default)]
    pub require_loaded: Option<bool>,
    #[serde(default)]
    pub lifecycle_state: Option<String>,
    #[serde(default)]
    pub load_state: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrainingConfig {
    #[serde(default)]
    pub docs_path: Option<String>,
    #[serde(default)]
    pub register_after_train: bool,
    #[serde(default)]
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatConfig {
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub determinism_mode: Option<String>,
    #[serde(default)]
    pub backend_profile: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub expected_text: Option<String>,
    #[serde(default)]
    pub probe_prompt: Option<String>,
    #[serde(default)]
    pub probe_max_tokens: Option<usize>,
    #[serde(default)]
    pub probe_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplayConfig {
    #[serde(default)]
    pub runs: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ScenarioLoader {
    root: PathBuf,
}

impl Default for ScenarioLoader {
    fn default() -> Self {
        Self::from_env()
    }
}

impl ScenarioLoader {
    pub fn from_env() -> Self {
        let root = env::var(ENV_SCENARIO_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_SCENARIO_DIR));
        Self { root }
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn list(&self) -> Result<Vec<ScenarioConfig>> {
        let mut configs = Vec::new();
        for path in self.scenario_paths()? {
            let cfg = Self::load_file(&path)?;
            configs.push(cfg);
        }
        configs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(configs)
    }

    pub fn load(&self, id: &str) -> Result<ScenarioConfig> {
        let wanted = ScenarioId::new(id);
        for path in self.scenario_paths()? {
            let cfg = Self::load_file(&path)?;
            if cfg.id == wanted {
                return Ok(cfg);
            }
        }
        Err(AosError::Config(format!(
            "Scenario '{}' not found in {}",
            id,
            self.root.display()
        )))
    }

    fn scenario_paths(&self) -> Result<Vec<PathBuf>> {
        if !self.root.exists() {
            return Err(AosError::Config(format!(
                "Scenario directory does not exist: {}",
                self.root.display()
            )));
        }

        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| {
            AosError::Io(format!(
                "Failed to read scenario directory {}: {}",
                self.root.display(),
                e
            ))
        })? {
            let entry = entry.map_err(|e| {
                AosError::Io(format!("Failed to read scenario directory entry: {}", e))
            })?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn load_file(path: &Path) -> Result<ScenarioConfig> {
        let content = fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read scenario file {}: {}",
                path.display(),
                e
            ))
        })?;
        toml::from_str::<ScenarioConfig>(&content).map_err(|e| {
            AosError::Config(format!(
                "Failed to parse scenario {}: {}",
                path.display(),
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_scenario(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(format!("{}.toml", name));
        let mut file = fs::File::create(&path).expect("create scenario");
        file.write_all(body.as_bytes()).expect("write scenario");
        path
    }

    #[test]
    fn loader_reads_default_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_scenario(
            dir.path(),
            "doc-chat",
            r#"
id = "doc-chat"
[tenant]
id = "tenant"
[model]
id = "model-1"
[adapter]
name = "adapter-a"
"#,
        );
        let loader = ScenarioLoader::with_root(dir.path());
        let list = loader.list().expect("list scenarios");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id.as_str(), "doc-chat");
    }

    #[test]
    fn loader_errors_on_missing_dir() {
        let loader = ScenarioLoader::with_root("/non-existent-scenarios");
        let err = loader.list().expect_err("should fail");
        assert!(format!("{err}").contains("Scenario directory"));
    }
}
