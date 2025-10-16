//! Framework detection engine for directory analysis.
//!
//! The implementation is intentionally heuristic-based – we are not
//! attempting to execute package managers or run heavy dependency
//! resolution.  Instead we inspect well-known configuration files and
//! directory layouts to infer which application framework(s) are in use.
//! The detector produces deterministic scores so that downstream routing
//! logic can make reproducible decisions.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Description of a detected framework.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedFramework {
    /// Framework display name (e.g. "React").
    pub name: String,
    /// Optional version extracted from dependency manifests.
    pub version: Option<String>,
    /// Confidence score between 0.0 and 1.0.
    pub confidence: f32,
    /// Routing rank.  Lower values indicate adapters that should fire
    /// earlier in the hierarchy (8–16 for framework adapters).
    pub rank: u8,
    /// Evidence collected during detection.
    pub evidence: Vec<String>,
}

/// Internal representation of the project metadata that we extract from the
/// repository.  All fields are optional and lazily populated.
#[derive(Default, Debug)]
struct ProjectMetadata {
    npm_dependencies: BTreeMap<String, String>,
    python_dependencies: BTreeMap<String, String>,
    cargo_dependencies: BTreeMap<String, String>,
    composer_dependencies: BTreeMap<String, String>,
    gem_dependencies: BTreeMap<String, String>,
    gradle_dependencies: BTreeSet<String>,
    maven_dependencies: BTreeSet<String>,
    config_files: BTreeSet<String>,
    directories: BTreeSet<PathBuf>,
}

impl ProjectMetadata {
    fn load(root: &Path) -> Result<Self> {
        let mut meta = Self::default();
        meta.ingest_directories(root)?;
        meta.parse_package_json(root);
        meta.parse_requirements(root);
        meta.parse_pyproject(root);
        meta.parse_cargo_toml(root);
        meta.parse_composer(root);
        meta.parse_gemfile(root);
        meta.parse_gradle(root);
        meta.parse_maven(root);
        Ok(meta)
    }

    fn ingest_directories(&mut self, root: &Path) -> Result<()> {
        let walker = walkdir::WalkDir::new(root)
            .min_depth(0)
            .max_depth(4)
            .follow_links(false);
        for entry in walker {
            let entry = entry.map_err(|e| AosError::Io(e.to_string()))?;
            let path = entry.path();
            if entry.file_type().is_dir() {
                if let Ok(rel) = path.strip_prefix(root) {
                    if !rel.as_os_str().is_empty() {
                        self.directories.insert(rel.to_path_buf());
                    }
                }
                continue;
            }

            if let Ok(rel) = path.strip_prefix(root) {
                if let Some(name) = rel.to_str() {
                    self.config_files.insert(name.replace('\\', "/"));
                }
            }
        }
        Ok(())
    }

    fn parse_package_json(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("package.json")) {
            self.config_files.insert("package.json".to_string());
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let deps = value
                    .get("dependencies")
                    .and_then(|v| v.as_object())
                    .into_iter()
                    .flat_map(|map| map.iter())
                    .chain(
                        value
                            .get("devDependencies")
                            .and_then(|v| v.as_object())
                            .into_iter()
                            .flat_map(|map| map.iter()),
                    );
                for (name, ver) in deps {
                    self.npm_dependencies
                        .insert(name.to_lowercase(), ver.as_str().unwrap_or("*").to_string());
                }
            }
        }
    }

    fn parse_requirements(&mut self, root: &Path) {
        for file in [
            "requirements.txt",
            "requirements-dev.txt",
            "dev-requirements.txt",
        ] {
            if let Some(text) = read_optional(root.join(file)) {
                self.config_files.insert(file.to_string());
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let (name, version) = line
                        .split_once(|c: char| c == '=' || c == '>' || c == '<')
                        .map(|(n, rest)| (n.trim(), Some(rest.trim().trim_start_matches('='))))
                        .unwrap_or((line, None));
                    self.python_dependencies
                        .insert(name.to_lowercase(), version.unwrap_or("*").to_string());
                }
            }
        }
    }

    fn parse_pyproject(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("pyproject.toml")) {
            self.config_files.insert("pyproject.toml".to_string());
            if let Ok(value) = text.parse::<toml::Value>() {
                if let Some(project) = value.get("project") {
                    if let Some(deps) = project.get("dependencies").and_then(|v| v.as_array()) {
                        for dep in deps {
                            if let Some(entry) = dep.as_str() {
                                let (name, version) = entry
                                    .split_once(|c: char| c == '=' || c == '>' || c == '<')
                                    .map(|(n, rest)| {
                                        (n.trim(), Some(rest.trim().trim_start_matches('=')))
                                    })
                                    .unwrap_or((entry, None));
                                self.python_dependencies.insert(
                                    name.to_lowercase(),
                                    version.unwrap_or("*").to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_cargo_toml(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("Cargo.toml")) {
            self.config_files.insert("Cargo.toml".to_string());
            if let Ok(value) = text.parse::<toml::Value>() {
                if let Some(deps) = value.get("dependencies").and_then(|v| v.as_table()) {
                    for (name, spec) in deps.iter() {
                        match spec {
                            toml::Value::String(ver) => {
                                self.cargo_dependencies
                                    .insert(name.to_lowercase(), ver.to_string());
                            }
                            toml::Value::Table(table) => {
                                if let Some(ver) = table.get("version").and_then(|v| v.as_str()) {
                                    self.cargo_dependencies
                                        .insert(name.to_lowercase(), ver.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn parse_composer(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("composer.json")) {
            self.config_files.insert("composer.json".to_string());
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let deps = value
                    .get("require")
                    .and_then(|v| v.as_object())
                    .into_iter()
                    .flat_map(|map| map.iter());
                for (name, ver) in deps {
                    self.composer_dependencies
                        .insert(name.to_lowercase(), ver.as_str().unwrap_or("*").to_string());
                }
            }
        }
    }

    fn parse_gemfile(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("Gemfile")) {
            self.config_files.insert("Gemfile".to_string());
            for line in text.lines() {
                let line = line.trim();
                if !line.starts_with("gem ") {
                    continue;
                }
                let tokens: Vec<_> = line
                    .split(',')
                    .map(|s| s.trim_matches(|c| c == '\'' || c == '"' || c.is_whitespace()))
                    .collect();
                if let Some(name) = tokens.get(0) {
                    let version = tokens
                        .get(1)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "*".into());
                    self.gem_dependencies.insert(name.to_string(), version);
                }
            }
        }
    }

    fn parse_gradle(&mut self, root: &Path) {
        for file in [
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ] {
            if let Some(text) = read_optional(root.join(file)) {
                self.config_files.insert(file.to_string());
                for line in text.lines() {
                    let cleaned = line.trim();
                    if cleaned.contains("org.springframework.boot") {
                        self.gradle_dependencies
                            .insert("org.springframework.boot".into());
                    }
                    if cleaned.contains("io.micronaut") {
                        self.gradle_dependencies.insert("io.micronaut".into());
                    }
                    if cleaned.contains("io.quarkus") {
                        self.gradle_dependencies.insert("io.quarkus".into());
                    }
                }
            }
        }
    }

    fn parse_maven(&mut self, root: &Path) {
        if let Some(text) = read_optional(root.join("pom.xml")) {
            self.config_files.insert("pom.xml".to_string());
            let lowered = text.to_lowercase();
            if lowered.contains("spring-boot-starter") {
                self.maven_dependencies.insert("spring-boot".into());
            }
            if lowered.contains("quarkus-") {
                self.maven_dependencies.insert("quarkus".into());
            }
        }
    }
}

/// Framework detection rule.  Each rule is deterministic and only depends on
/// `ProjectMetadata`.
struct FrameworkRule {
    name: &'static str,
    rank: u8,
    indicators: Vec<Indicator>,
    keywords: &'static [&'static str],
}

#[derive(Clone)]
enum Indicator {
    Npm(&'static [&'static str]),
    Python(&'static [&'static str]),
    Cargo(&'static [&'static str]),
    Composer(&'static [&'static str]),
    Gem(&'static [&'static str]),
    Gradle(&'static [&'static str]),
    Maven(&'static [&'static str]),
    Config(&'static [&'static str]),
    Directory(&'static [&'static str]),
}

/// Detect frameworks present in the project directory.
pub fn detect_frameworks(root: &Path) -> Result<Vec<DetectedFramework>> {
    let metadata = ProjectMetadata::load(root)?;

    let rules = framework_rules();
    let mut detections = Vec::new();

    for rule in rules {
        let mut evidence = Vec::new();
        let mut score = 0.0f32;
        let mut version: Option<String> = None;

        for indicator in &rule.indicators {
            match indicator {
                Indicator::Npm(pkgs) => {
                    for pkg in *pkgs {
                        if let Some(ver) = metadata.npm_dependencies.get(&pkg.to_lowercase()) {
                            evidence.push(format!("npm:{}@{}", pkg, ver));
                            version = version.clone().or_else(|| Some(ver.clone()));
                            score += 0.25;
                        }
                    }
                }
                Indicator::Python(pkgs) => {
                    for pkg in *pkgs {
                        if let Some(ver) = metadata.python_dependencies.get(&pkg.to_lowercase()) {
                            evidence.push(format!("python:{}@{}", pkg, ver));
                            version = version.clone().or_else(|| Some(ver.clone()));
                            score += 0.25;
                        }
                    }
                }
                Indicator::Cargo(pkgs) => {
                    for pkg in *pkgs {
                        if let Some(ver) = metadata.cargo_dependencies.get(&pkg.to_lowercase()) {
                            evidence.push(format!("cargo:{}@{}", pkg, ver));
                            version = version.clone().or_else(|| Some(ver.clone()));
                            score += 0.25;
                        }
                    }
                }
                Indicator::Composer(pkgs) => {
                    for pkg in *pkgs {
                        if let Some(ver) = metadata.composer_dependencies.get(&pkg.to_lowercase()) {
                            evidence.push(format!("composer:{}@{}", pkg, ver));
                            version = version.clone().or_else(|| Some(ver.clone()));
                            score += 0.25;
                        }
                    }
                }
                Indicator::Gem(pkgs) => {
                    for pkg in *pkgs {
                        if let Some(ver) = metadata.gem_dependencies.get(*pkg) {
                            evidence.push(format!("gem:{}@{}", pkg, ver));
                            version = version.clone().or_else(|| Some(ver.clone()));
                            score += 0.25;
                        }
                    }
                }
                Indicator::Gradle(pkgs) => {
                    for pkg in *pkgs {
                        if metadata.gradle_dependencies.contains(*pkg) {
                            evidence.push(format!("gradle:{}", pkg));
                            score += 0.2;
                        }
                    }
                }
                Indicator::Maven(pkgs) => {
                    for pkg in *pkgs {
                        if metadata.maven_dependencies.contains(*pkg) {
                            evidence.push(format!("maven:{}", pkg));
                            score += 0.2;
                        }
                    }
                }
                Indicator::Config(files) => {
                    for file in *files {
                        if metadata.config_files.contains(&file.to_string()) {
                            evidence.push(format!("config:{}", file));
                            score += 0.15;
                        }
                    }
                }
                Indicator::Directory(paths) => {
                    for dir in *paths {
                        if metadata
                            .directories
                            .iter()
                            .any(|p| p.to_string_lossy().contains(dir))
                        {
                            evidence.push(format!("dir:{}", dir));
                            score += 0.1;
                        }
                    }
                }
            }
        }

        if score >= 0.4 {
            let capped = score.min(1.0);
            detections.push(DetectedFramework {
                name: rule.name.to_string(),
                version,
                confidence: (capped * 100.0).round() / 100.0,
                rank: rule.rank,
                evidence,
            });
        }
    }

    detections.sort_by(|a, b| a.rank.cmp(&b.rank).then_with(|| a.name.cmp(&b.name)));
    debug!("detected_frameworks = {:?}", detections);
    Ok(detections)
}

fn read_optional(path: PathBuf) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn framework_rules() -> Vec<FrameworkRule> {
    vec![
        FrameworkRule {
            name: "React",
            rank: 9,
            indicators: vec![
                Indicator::Npm(&["react", "react-dom"]),
                Indicator::Config(&["package.json", "tsconfig.json"]),
                Indicator::Directory(&["src/components", "src/hooks"]),
            ],
            keywords: &["jsx", "tsx", "useState", "useEffect"],
        },
        FrameworkRule {
            name: "Next.js",
            rank: 10,
            indicators: vec![
                Indicator::Npm(&["next"]),
                Indicator::Config(&["next.config.js", "next.config.mjs", "next.config.ts"]),
                Indicator::Directory(&["pages", "app"]),
            ],
            keywords: &["getServerSideProps", "metadata", "next"],
        },
        FrameworkRule {
            name: "Vue",
            rank: 11,
            indicators: vec![
                Indicator::Npm(&["vue", "@vue/runtime-core"]),
                Indicator::Config(&["vue.config.js"]),
            ],
            keywords: &["vue", "template", "setup"],
        },
        FrameworkRule {
            name: "Angular",
            rank: 12,
            indicators: vec![
                Indicator::Npm(&["@angular/core"]),
                Indicator::Config(&["angular.json"]),
                Indicator::Directory(&["src/app"]),
            ],
            keywords: &["ngOnInit", "@Component", "Angular"],
        },
        FrameworkRule {
            name: "Express",
            rank: 13,
            indicators: vec![
                Indicator::Npm(&["express"]),
                Indicator::Config(&["package.json"]),
            ],
            keywords: &["app.get", "express.Router"],
        },
        FrameworkRule {
            name: "Django",
            rank: 8,
            indicators: vec![
                Indicator::Python(&["django"]),
                Indicator::Config(&["manage.py", "settings.py"]),
                Indicator::Directory(&["migrations", "templates"]),
            ],
            keywords: &["django", "urlpatterns", "QuerySet"],
        },
        FrameworkRule {
            name: "FastAPI",
            rank: 14,
            indicators: vec![
                Indicator::Python(&["fastapi"]),
                Indicator::Config(&["requirements.txt", "pyproject.toml"]),
            ],
            keywords: &["FastAPI", "APIRouter", "Depends"],
        },
        FrameworkRule {
            name: "Flask",
            rank: 15,
            indicators: vec![
                Indicator::Python(&["flask"]),
                Indicator::Config(&["requirements.txt", "pyproject.toml"]),
            ],
            keywords: &["Flask", "@app.route"],
        },
        FrameworkRule {
            name: "Rails",
            rank: 8,
            indicators: vec![
                Indicator::Gem(&["rails"]),
                Indicator::Config(&["config/routes.rb", "config/application.rb"]),
            ],
            keywords: &["ActiveRecord", "rails", "ActionController"],
        },
        FrameworkRule {
            name: "Laravel",
            rank: 12,
            indicators: vec![
                Indicator::Composer(&["laravel/framework"]),
                Indicator::Config(&["artisan", "composer.json"]),
            ],
            keywords: &["Illuminate", "ServiceProvider"],
        },
        FrameworkRule {
            name: "Spring Boot",
            rank: 16,
            indicators: vec![
                Indicator::Gradle(&["org.springframework.boot"]),
                Indicator::Maven(&["spring-boot"]),
            ],
            keywords: &["@SpringBootApplication", "RestController"],
        },
        FrameworkRule {
            name: "Quarkus",
            rank: 15,
            indicators: vec![
                Indicator::Gradle(&["io.quarkus"]),
                Indicator::Maven(&["quarkus"]),
            ],
            keywords: &["@ApplicationScoped", "Quarkus"],
        },
        FrameworkRule {
            name: "Actix Web",
            rank: 14,
            indicators: vec![Indicator::Cargo(&["actix-web", "actix"])],
            keywords: &["actix_web::", "HttpServer"],
        },
        FrameworkRule {
            name: "Axum",
            rank: 14,
            indicators: vec![
                Indicator::Cargo(&["axum"]),
                Indicator::Config(&["Cargo.toml"]),
            ],
            keywords: &["axum::", "Router::new"],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(dir: &Path, path: &str, contents: &str) {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(full_path, contents).unwrap();
    }

    #[test]
    fn detects_react_project() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "package.json",
            r#"{"name":"demo","dependencies":{"react":"18.2.0","react-dom":"18.2.0"}}"#,
        );
        write_file(
            dir.path(),
            "src/components/Button.tsx",
            "export const Button = () => null;",
        );

        let frameworks = detect_frameworks(dir.path()).unwrap();
        assert!(frameworks.iter().any(|f| f.name == "React"));
    }

    #[test]
    fn detects_django_project() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "requirements.txt", "django==4.1\n");
        write_file(dir.path(), "manage.py", "#!/usr/bin/env python\n");

        let frameworks = detect_frameworks(dir.path()).unwrap();
        let django = frameworks.iter().find(|f| f.name == "Django").unwrap();
        assert!(django.confidence >= 0.7);
    }

    #[test]
    fn detects_rails_and_laravel_independently() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "Gemfile", "gem 'rails', '7.0'\n");
        write_file(
            dir.path(),
            "composer.json",
            "{\"require\":{\"laravel/framework\":\"10.0\"}}",
        );

        let frameworks = detect_frameworks(dir.path()).unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Rails"));
        assert!(frameworks.iter().any(|f| f.name == "Laravel"));
    }

    #[test]
    fn detects_spring_boot_from_gradle() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "build.gradle",
            "plugins { id 'org.springframework.boot' version '3.1.0' }",
        );

        let frameworks = detect_frameworks(dir.path()).unwrap();
        assert!(frameworks.iter().any(|f| f.name == "Spring Boot"));
    }
}
