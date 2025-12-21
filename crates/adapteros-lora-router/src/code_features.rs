//! Code-specific feature extraction for router scoring

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Extracts features from code changes for router scoring
pub struct CodeFeatureExtractor;

impl CodeFeatureExtractor {
    /// Extract language distribution from file paths
    /// Returns a normalized vector: [rust, python, typescript, javascript, other]
    pub fn extract_language_features(files: &[PathBuf]) -> Vec<f32> {
        let mut features = vec![0.0; 5];

        if files.is_empty() {
            return features;
        }

        for file in files {
            if let Some(ext) = file.extension().and_then(|e| e.to_str()) {
                match ext {
                    "rs" => features[0] += 1.0,
                    "py" => features[1] += 1.0,
                    "ts" | "tsx" => features[2] += 1.0,
                    "js" | "jsx" => features[3] += 1.0,
                    _ => features[4] += 1.0,
                }
            } else {
                features[4] += 1.0; // No extension
            }
        }

        // Normalize to sum to 1.0
        let sum: f32 = features.iter().sum();
        if sum > 0.0 {
            features.iter_mut().for_each(|f| *f /= sum);
        }

        features
    }

    /// Extract framework adapter boost scores
    /// Returns map of framework name -> boost multiplier
    pub fn extract_framework_priors(frameworks: &HashSet<String>) -> HashMap<String, f32> {
        let mut priors = HashMap::new();

        // Define boost scores for different frameworks
        let boost_mapping = [
            // Python frameworks
            ("django", 2.0),
            ("fastapi", 2.0),
            ("flask", 1.8),
            ("pytest", 1.5),
            ("sqlalchemy", 1.7),
            ("celery", 1.6),
            // Rust frameworks
            ("axum", 2.0),
            ("tokio", 1.8),
            ("actix-web", 2.0),
            // JavaScript/TypeScript frameworks
            ("react", 2.0),
            ("nextjs", 2.2),
            ("vue", 1.9),
            ("angular", 2.0),
            ("express", 1.8),
            // Infrastructure
            ("kubernetes", 1.7),
            ("terraform", 1.6),
        ];

        for framework in frameworks {
            let framework_lower = framework.to_lowercase();
            for (name, boost) in &boost_mapping {
                if framework_lower.contains(name) {
                    priors.insert(framework.clone(), *boost);
                    break;
                }
            }
        }

        priors
    }

    /// Extract symbol hit score
    /// Returns a float representing how many symbols changed (capped at 100)
    pub fn extract_symbol_hits(changed_symbols: &[String]) -> f32 {
        let count = changed_symbols.len() as f32;
        // Cap at 100 and normalize to 0-1 range
        count.min(100.0) / 100.0
    }

    /// Extract path tokens for pattern matching
    /// Returns unique directory and file name tokens
    pub fn extract_path_tokens(files: &[PathBuf]) -> Vec<String> {
        let mut tokens = HashSet::new();

        for file in files {
            // Add each path component as a token
            for component in file.components() {
                if let Some(s) = component.as_os_str().to_str() {
                    if !s.is_empty() && s != "." && s != ".." {
                        tokens.insert(s.to_string());
                    }
                }
            }

            // Add filename without extension
            if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
                tokens.insert(stem.to_string());
            }
        }

        tokens.into_iter().collect()
    }

    /// Extract module depth score (deeper paths may indicate more complex changes)
    pub fn extract_depth_score(files: &[PathBuf]) -> f32 {
        if files.is_empty() {
            return 0.0;
        }

        let total_depth: usize = files.iter().map(|f| f.components().count()).sum();

        let avg_depth = total_depth as f32 / files.len() as f32;

        // Normalize: typical depth 2-5, cap at 10
        (avg_depth / 10.0).min(1.0)
    }

    /// Check if changes involve test files
    pub fn is_test_related(files: &[PathBuf]) -> bool {
        files.iter().any(|f| {
            let path_str = f.to_string_lossy().to_lowercase();
            path_str.contains("test") || path_str.contains("spec")
        })
    }

    /// Check if changes involve configuration files
    pub fn is_config_related(files: &[PathBuf]) -> bool {
        files.iter().any(|f| {
            let path_str = f.to_string_lossy().to_lowercase();
            path_str.contains("config")
                || path_str.ends_with(".toml")
                || path_str.ends_with(".yaml")
                || path_str.ends_with(".yml")
                || path_str.ends_with(".json")
                || path_str.ends_with(".env")
        })
    }

    /// Extract commit scope features (file count, lines changed estimate)
    pub fn extract_scope_features(file_count: usize, symbol_count: usize) -> (f32, f32) {
        // Normalize file count (0-20 files is typical, cap at 50)
        let file_score = (file_count as f32 / 50.0).min(1.0);

        // Normalize symbol count (0-50 symbols is typical, cap at 200)
        let symbol_score = (symbol_count as f32 / 200.0).min(1.0);

        (file_score, symbol_score)
    }
}

/// Complete feature set for code routing
#[derive(Debug, Clone)]
pub struct CodeFeatures {
    pub language_dist: Vec<f32>,
    pub framework_priors: HashMap<String, f32>,
    pub symbol_hit_score: f32,
    pub path_tokens: Vec<String>,
    pub depth_score: f32,
    pub is_test: bool,
    pub is_config: bool,
    pub file_scope: f32,
    pub symbol_scope: f32,
}

impl CodeFeatures {
    /// Extract all features from code change metadata
    pub fn extract(
        files: &[PathBuf],
        frameworks: &HashSet<String>,
        changed_symbols: &[String],
    ) -> Self {
        let (file_scope, symbol_scope) =
            CodeFeatureExtractor::extract_scope_features(files.len(), changed_symbols.len());

        Self {
            language_dist: CodeFeatureExtractor::extract_language_features(files),
            framework_priors: CodeFeatureExtractor::extract_framework_priors(frameworks),
            symbol_hit_score: CodeFeatureExtractor::extract_symbol_hits(changed_symbols),
            path_tokens: CodeFeatureExtractor::extract_path_tokens(files),
            depth_score: CodeFeatureExtractor::extract_depth_score(files),
            is_test: CodeFeatureExtractor::is_test_related(files),
            is_config: CodeFeatureExtractor::is_config_related(files),
            file_scope,
            symbol_scope,
        }
    }

    /// Get boost score for a specific adapter based on its name/type
    pub fn get_adapter_boost(&self, adapter_name: &str) -> f32 {
        let name_lower = adapter_name.to_lowercase();

        // Check framework priors
        for (framework, boost) in &self.framework_priors {
            if name_lower.contains(&framework.to_lowercase()) {
                return *boost;
            }
        }

        // Check language matches
        if name_lower.contains("rust") && self.language_dist[0] > 0.3 {
            return 1.5;
        }
        if name_lower.contains("python") && self.language_dist[1] > 0.3 {
            return 1.5;
        }
        if name_lower.contains("typescript") && self.language_dist[2] > 0.3 {
            return 1.5;
        }
        if name_lower.contains("javascript") && self.language_dist[3] > 0.3 {
            return 1.4;
        }

        // Check for test/config adapters
        if name_lower.contains("test") && self.is_test {
            return 1.3;
        }
        if name_lower.contains("config") && self.is_config {
            return 1.2;
        }

        // Default: no boost
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_language_features() {
        let files = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("script.py"),
            PathBuf::from("app.ts"),
        ];

        let features = CodeFeatureExtractor::extract_language_features(&files);

        // Should be normalized
        let sum: f32 = features.iter().sum();
        assert!((sum - 1.0).abs() < 0.01);

        // Check individual proportions
        assert!((features[0] - 0.5).abs() < 0.01); // 2/4 = 0.5 for Rust
        assert!((features[1] - 0.25).abs() < 0.01); // 1/4 = 0.25 for Python
        assert!((features[2] - 0.25).abs() < 0.01); // 1/4 = 0.25 for TypeScript
    }

    #[test]
    fn test_extract_language_features_empty() {
        let files: Vec<PathBuf> = vec![];
        let features = CodeFeatureExtractor::extract_language_features(&files);
        assert_eq!(features, vec![0.0; 5]);
    }

    #[test]
    fn test_extract_framework_priors() {
        let mut frameworks = HashSet::new();
        frameworks.insert("Django".to_string());
        frameworks.insert("React".to_string());

        let priors = CodeFeatureExtractor::extract_framework_priors(&frameworks);

        assert_eq!(priors.get("Django"), Some(&2.0));
        assert_eq!(priors.get("React"), Some(&2.0));
    }

    #[test]
    fn test_extract_symbol_hits() {
        let symbols = vec![
            "func1".to_string(),
            "func2".to_string(),
            "func3".to_string(),
        ];
        let score = CodeFeatureExtractor::extract_symbol_hits(&symbols);
        assert!((score - 0.03).abs() < 0.01); // 3/100 = 0.03
    }

    #[test]
    fn test_extract_symbol_hits_capped() {
        let symbols: Vec<String> = (0..150).map(|i| format!("func{}", i)).collect();
        let score = CodeFeatureExtractor::extract_symbol_hits(&symbols);
        assert_eq!(score, 1.0); // Capped at 100, so 100/100 = 1.0
    }

    #[test]
    fn test_extract_path_tokens() {
        let files = vec![
            PathBuf::from("src/api/handlers.rs"),
            PathBuf::from("tests/integration_test.py"),
        ];

        let tokens = CodeFeatureExtractor::extract_path_tokens(&files);

        assert!(tokens.contains(&"src".to_string()));
        assert!(tokens.contains(&"api".to_string()));
        assert!(tokens.contains(&"handlers".to_string()));
        assert!(tokens.contains(&"tests".to_string()));
        assert!(tokens.contains(&"integration_test".to_string()));
    }

    #[test]
    fn test_extract_depth_score() {
        let files = vec![
            PathBuf::from("a/b/c/d/e.rs"), // depth 5
            PathBuf::from("x/y.py"),       // depth 2
        ];

        let score = CodeFeatureExtractor::extract_depth_score(&files);
        // Average depth: (5 + 2) / 2 = 3.5, normalized: 3.5/10 = 0.35
        assert!((score - 0.35).abs() < 0.01);
    }

    #[test]
    fn test_is_test_related() {
        let test_files = vec![PathBuf::from("tests/test_api.py")];
        assert!(CodeFeatureExtractor::is_test_related(&test_files));

        let non_test_files = vec![PathBuf::from("src/main.rs")];
        assert!(!CodeFeatureExtractor::is_test_related(&non_test_files));
    }

    #[test]
    fn test_is_config_related() {
        let config_files = vec![PathBuf::from("config.yaml")];
        assert!(CodeFeatureExtractor::is_config_related(&config_files));

        let non_config_files = vec![PathBuf::from("src/main.rs")];
        assert!(!CodeFeatureExtractor::is_config_related(&non_config_files));
    }

    #[test]
    fn test_extract_scope_features() {
        let (file_score, symbol_score) = CodeFeatureExtractor::extract_scope_features(10, 25);

        assert!((file_score - 0.2).abs() < 0.01); // 10/50 = 0.2
        assert!((symbol_score - 0.125).abs() < 0.01); // 25/200 = 0.125
    }

    #[test]
    fn test_code_features_extract() {
        let files = vec![PathBuf::from("src/main.rs"), PathBuf::from("tests/test.py")];

        let mut frameworks = HashSet::new();
        frameworks.insert("pytest".to_string());

        let symbols = vec!["func1".to_string(), "func2".to_string()];

        let features = CodeFeatures::extract(&files, &frameworks, &symbols);

        assert!(!features.language_dist.is_empty());
        assert_eq!(features.framework_priors.get("pytest"), Some(&1.5));
        assert!(features.is_test);
        // Path tokens: src, main, tests, test, plus file stems (main, test)
        assert!(
            features.path_tokens.len() >= 4,
            "Expected at least 4 tokens, got {}",
            features.path_tokens.len()
        );
    }

    #[test]
    fn test_get_adapter_boost() {
        let files = vec![PathBuf::from("src/main.rs")];
        let mut frameworks = HashSet::new();
        frameworks.insert("django".to_string());
        let symbols = vec![];

        let features = CodeFeatures::extract(&files, &frameworks, &symbols);

        // Django adapter should get boost
        assert_eq!(features.get_adapter_boost("python_django_v1"), 2.0);

        // Rust adapter should get boost (>30% rust files)
        assert_eq!(features.get_adapter_boost("code_rust_v1"), 1.5);

        // Unrelated adapter should get no boost
        assert_eq!(features.get_adapter_boost("java_spring_v1"), 1.0);
    }

    #[test]
    fn test_get_adapter_boost_test_adapter() {
        let files = vec![PathBuf::from("tests/test_api.py")];
        let frameworks = HashSet::new();
        let symbols = vec![];

        let features = CodeFeatures::extract(&files, &frameworks, &symbols);

        assert_eq!(features.get_adapter_boost("test_framework_v1"), 1.3);
    }
}
