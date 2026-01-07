//! Language detection and test framework identification

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Detected language and test framework
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Primary language
    pub language: String,
    /// Test framework to use
    pub test_framework: TestFramework,
    /// Linter to use
    pub linter: Linter,
    /// Test command arguments
    pub test_command: Vec<String>,
    /// Linter command arguments
    pub linter_command: Vec<String>,
}

/// Test framework enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestFramework {
    CargoTest,
    CargoNextest,
    Pytest,
    NpmTest,
    Jest,
    GoTest,
    MavenTest,
    GradleTest,
}

/// Linter enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Linter {
    Clippy,
    Rustfmt,
    Ruff,
    Black,
    Eslint,
    Prettier,
    GoFmt,
    GoVet,
    Checkstyle,
    Spotbugs,
}

/// Language detector
pub struct LanguageDetector {
    repo_path: PathBuf,
}

impl LanguageDetector {
    /// Create a new language detector
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Detect languages and frameworks for the repository
    pub fn detect(&self) -> Result<Vec<LanguageConfig>> {
        let mut configs = Vec::new();

        // Detect Rust
        if let Some(rust_config) = self.detect_rust()? {
            configs.push(rust_config);
        }

        // Detect Python
        if let Some(python_config) = self.detect_python()? {
            configs.push(python_config);
        }

        // Detect JavaScript/TypeScript
        if let Some(js_config) = self.detect_javascript()? {
            configs.push(js_config);
        }

        // Detect Go
        if let Some(go_config) = self.detect_go()? {
            configs.push(go_config);
        }

        // Detect Java
        if let Some(java_config) = self.detect_java()? {
            configs.push(java_config);
        }

        Ok(configs)
    }

    /// Detect Rust configuration
    fn detect_rust(&self) -> Result<Option<LanguageConfig>> {
        let cargo_toml = self.repo_path.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Ok(None);
        }

        // Check for cargo-nextest
        let has_nextest = self.has_cargo_nextest()?;

        Ok(Some(LanguageConfig {
            language: "rust".to_string(),
            test_framework: if has_nextest {
                TestFramework::CargoNextest
            } else {
                TestFramework::CargoTest
            },
            linter: Linter::Clippy,
            test_command: if has_nextest {
                vec![
                    "cargo".to_string(),
                    "nextest".to_string(),
                    "run".to_string(),
                ]
            } else {
                vec!["cargo".to_string(), "test".to_string()]
            },
            linter_command: vec![
                "cargo".to_string(),
                "clippy".to_string(),
                "--".to_string(),
                "-D".to_string(),
                "warnings".to_string(),
            ],
        }))
    }

    /// Detect Python configuration
    fn detect_python(&self) -> Result<Option<LanguageConfig>> {
        let pyproject_toml = self.repo_path.join("pyproject.toml");
        let requirements_txt = self.repo_path.join("requirements.txt");
        let setup_py = self.repo_path.join("setup.py");

        if !pyproject_toml.exists() && !requirements_txt.exists() && !setup_py.exists() {
            return Ok(None);
        }

        // Check for pytest
        let has_pytest = self.has_pytest()?;

        Ok(Some(LanguageConfig {
            language: "python".to_string(),
            test_framework: TestFramework::Pytest,
            linter: Linter::Ruff,
            test_command: if has_pytest {
                vec!["pytest".to_string(), "--tb=short".to_string()]
            } else {
                vec![
                    "python".to_string(),
                    "-m".to_string(),
                    "unittest".to_string(),
                ]
            },
            linter_command: vec!["ruff".to_string(), "check".to_string()],
        }))
    }

    /// Detect JavaScript/TypeScript configuration
    fn detect_javascript(&self) -> Result<Option<LanguageConfig>> {
        let package_json = self.repo_path.join("package.json");
        if !package_json.exists() {
            return Ok(None);
        }

        // Check for Jest
        let has_jest = self.has_jest()?;

        Ok(Some(LanguageConfig {
            language: "javascript".to_string(),
            test_framework: if has_jest {
                TestFramework::Jest
            } else {
                TestFramework::NpmTest
            },
            linter: Linter::Eslint,
            test_command: if has_jest {
                vec!["npx".to_string(), "jest".to_string()]
            } else {
                vec!["npm".to_string(), "test".to_string()]
            },
            linter_command: vec!["npx".to_string(), "eslint".to_string(), ".".to_string()],
        }))
    }

    /// Detect Go configuration
    fn detect_go(&self) -> Result<Option<LanguageConfig>> {
        let go_mod = self.repo_path.join("go.mod");
        if !go_mod.exists() {
            return Ok(None);
        }

        Ok(Some(LanguageConfig {
            language: "go".to_string(),
            test_framework: TestFramework::GoTest,
            linter: Linter::GoVet,
            test_command: vec!["go".to_string(), "test".to_string(), "./...".to_string()],
            linter_command: vec!["go".to_string(), "vet".to_string(), "./...".to_string()],
        }))
    }

    /// Detect Java configuration
    fn detect_java(&self) -> Result<Option<LanguageConfig>> {
        let pom_xml = self.repo_path.join("pom.xml");
        let build_gradle = self.repo_path.join("build.gradle");

        if !pom_xml.exists() && !build_gradle.exists() {
            return Ok(None);
        }

        let (test_framework, test_command, linter_command) = if pom_xml.exists() {
            (
                TestFramework::MavenTest,
                vec!["mvn".to_string(), "test".to_string()],
                vec!["mvn".to_string(), "checkstyle:check".to_string()],
            )
        } else {
            (
                TestFramework::GradleTest,
                vec!["./gradlew".to_string(), "test".to_string()],
                vec!["./gradlew".to_string(), "checkstyleMain".to_string()],
            )
        };

        Ok(Some(LanguageConfig {
            language: "java".to_string(),
            test_framework,
            linter: Linter::Checkstyle,
            test_command,
            linter_command,
        }))
    }

    /// Check if cargo-nextest is available
    fn has_cargo_nextest(&self) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("cargo")
            .arg("nextest")
            .arg("--version")
            .current_dir(&self.repo_path)
            .output();

        Ok(output.is_ok() && output.unwrap().status.success())
    }

    /// Check if pytest is available
    fn has_pytest(&self) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("pytest")
            .arg("--version")
            .current_dir(&self.repo_path)
            .output();

        Ok(output.is_ok() && output.unwrap().status.success())
    }

    /// Check if Jest is available
    fn has_jest(&self) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("npx")
            .arg("jest")
            .arg("--version")
            .current_dir(&self.repo_path)
            .output();

        Ok(output.is_ok() && output.unwrap().status.success())
    }

    /// Get file extensions for a language
    pub fn get_extensions_for_language(language: &str) -> Vec<&'static str> {
        match language {
            "rust" => vec!["rs"],
            "python" => vec!["py"],
            "javascript" => vec!["js", "jsx"],
            "typescript" => vec!["ts", "tsx"],
            "go" => vec!["go"],
            "java" => vec!["java"],
            "cpp" => vec!["cpp", "cc", "cxx"],
            "c" => vec!["c"],
            _ => vec![],
        }
    }

    /// Detect language from file extension
    pub fn detect_language_from_extension(extension: &str) -> Option<String> {
        match extension {
            "rs" => Some("rust".to_string()),
            "py" => Some("python".to_string()),
            "js" | "jsx" => Some("javascript".to_string()),
            "ts" | "tsx" => Some("typescript".to_string()),
            "go" => Some("go".to_string()),
            "java" => Some("java".to_string()),
            "cpp" | "cc" | "cxx" => Some("cpp".to_string()),
            "c" => Some("c".to_string()),
            _ => None,
        }
    }

    /// Get test directory patterns for a language
    pub fn get_test_patterns_for_language(language: &str) -> Vec<&'static str> {
        match language {
            "rust" => vec!["tests/", "src/tests/", "tests.rs"],
            "python" => vec!["tests/", "test_", "_test.py"],
            "javascript" | "typescript" => vec!["__tests__/", "test/", ".test.", ".spec."],
            "go" => vec!["_test.go"],
            "java" => vec!["src/test/", "Test.java"],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("create temp dir")
    }

    #[test]
    fn test_language_detector_creation() {
        let temp_dir = new_test_tempdir();
        let detector = LanguageDetector::new(temp_dir.path());
        assert_eq!(detector.repo_path, temp_dir.path());
    }

    #[test]
    fn test_detect_language_from_extension() {
        assert_eq!(
            LanguageDetector::detect_language_from_extension("rs"),
            Some("rust".to_string())
        );

        assert_eq!(
            LanguageDetector::detect_language_from_extension("py"),
            Some("python".to_string())
        );

        assert_eq!(
            LanguageDetector::detect_language_from_extension("js"),
            Some("javascript".to_string())
        );

        assert_eq!(
            LanguageDetector::detect_language_from_extension("ts"),
            Some("typescript".to_string())
        );

        assert_eq!(
            LanguageDetector::detect_language_from_extension("go"),
            Some("go".to_string())
        );

        assert_eq!(
            LanguageDetector::detect_language_from_extension("java"),
            Some("java".to_string())
        );

        assert_eq!(LanguageDetector::detect_language_from_extension("md"), None);
    }

    #[test]
    fn test_get_extensions_for_language() {
        assert_eq!(
            LanguageDetector::get_extensions_for_language("rust"),
            vec!["rs"]
        );

        assert_eq!(
            LanguageDetector::get_extensions_for_language("python"),
            vec!["py"]
        );

        assert_eq!(
            LanguageDetector::get_extensions_for_language("javascript"),
            vec!["js", "jsx"]
        );

        assert_eq!(
            LanguageDetector::get_extensions_for_language("typescript"),
            vec!["ts", "tsx"]
        );
    }

    #[test]
    fn test_get_test_patterns_for_language() {
        assert_eq!(
            LanguageDetector::get_test_patterns_for_language("rust"),
            vec!["tests/", "src/tests/", "tests.rs"]
        );

        assert_eq!(
            LanguageDetector::get_test_patterns_for_language("python"),
            vec!["tests/", "test_", "_test.py"]
        );

        assert_eq!(
            LanguageDetector::get_test_patterns_for_language("javascript"),
            vec!["__tests__/", "test/", ".test.", ".spec."]
        );
    }
}
