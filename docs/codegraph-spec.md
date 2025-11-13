# CodeGraph Specification

## Overview

The **CodeGraph** provides production-grade framework detection and repository metadata analysis for AdapterOS. It implements heuristic analysis of dependency manifests and directory structures to identify frameworks, languages, and security issues with deterministic results suitable for adapter routing decisions.

**Status: ✅ IMPLEMENTED** - Available via `/api/v1/codegraph/frameworks/detect` and `/api/v1/codegraph/repository/metadata` endpoints.

## Goals

1. **Framework Detection**: Identify 15+ frameworks (React, Django, Rails, Spring Boot, etc.) from dependency manifests
2. **Language Analysis**: Count files and lines by programming language
3. **Security Scanning**: Entropy-based secret detection with configurable severity thresholds
4. **Git Integration**: Efficient repository statistics without expensive commit walking
5. **Performance**: Sub-second analysis with 5-minute caching for repeated requests

## API Structures

### Framework Detection

#### Request
```json
{
  "path": "/absolute/path/to/directory",
  "framework_types": ["React", "Django"]  // optional filter
}
```

#### Response
```json
{
  "frameworks": [
    {
      "name": "React",
      "version": "18.2.0",
      "confidence": 0.95,
      "rank": 9,
      "evidence": ["npm:react@18.2.0", "npm:react-dom@18.2.0"]
    }
  ],
  "timestamp": "2025-11-13T02:14:14.000Z",
  "analysis_time_ms": 45
}
```

### Repository Metadata

#### Request
```json
{
  "path": "/absolute/path/to/repository",
  "include_frameworks": true,
  "include_languages": true,
  "include_security": true
}
```

#### Response
```json
{
  "path": "/absolute/path/to/repository",
  "frameworks": [...],
  "languages": [
    {
      "name": "TypeScript",
      "files": 45,
      "lines": 12340,
      "percentage": 67.2
    }
  ],
  "security": {
    "violations": [
      {
        "file_path": "config/database.js",
        "pattern": "hardcoded_password",
        "line_number": 15,
        "severity": "high"
      }
    ],
    "scan_timestamp": "2025-11-13T02:14:14.000Z",
    "status": "completed"
  },
  "git_info": {
    "branch": "main",
    "commit_count": 1250,
    "last_commit": "a1b2c3d4...",
    "authors": ["Alice", "Bob", "Charlie"]
  },
  "timestamp": "2025-11-13T02:14:14.000Z",
  "analysis_time_ms": 125
}
```

## Implementation Details

### Framework Detection Engine

The framework detector analyzes dependency manifests and directory structures:

```rust
// Core detection logic from adapteros_codegraph
pub fn detect_frameworks(root: &Path) -> Result<Vec<DetectedFramework>> {
    let metadata = ProjectMetadata::load(root)?;
    let rules = framework_rules();
    let mut detections = Vec::new();

    for rule in rules {
        let mut evidence = Vec::new();
        let mut score = 0.0f32;

        // Check npm, Python, Cargo, etc. dependencies
        for indicator in &rule.indicators {
            match indicator {
                Indicator::Npm(pkgs) => {
                    for pkg in *pkgs {
                        if metadata.npm_dependencies.contains_key(&pkg.to_lowercase()) {
                            evidence.push(format!("npm:{}", pkg));
                            score += 0.25;
                        }
                    }
                }
                // Similar for Python, Cargo, Composer, Gem, etc.
            }
        }

        if score >= 0.3 {
            detections.push(DetectedFramework {
                name: rule.name.to_string(),
                confidence: (score.min(1.0) * 100.0).round() / 100.0,
                rank: rule.rank,
                evidence,
            });
        }
    }

    Ok(detections)
}
```

### Supported Frameworks

The detector identifies 15+ frameworks with confidence scoring:

- **Frontend**: React, Next.js, Vue, Angular, Express
- **Backend**: Django, FastAPI, Flask, Rails, Laravel, Spring Boot, Quarkus
- **Systems**: Actix Web, Axum
- **Full Stack**: Django, Rails, Laravel, Spring Boot

### Language Analysis

Counts files and lines by extension-based language detection:
- Rust (.rs), Python (.py), JavaScript (.js), TypeScript (.ts)
- Java (.java), Go (.go), C/C++ (.c/.cpp), PHP (.php), Ruby (.rb)
- Configuration: YAML, JSON, TOML, XML

## Security Scanning

### Entropy-Based Secret Detection

The security scanner uses Shannon entropy to identify potential secrets:

```rust
/// Calculate entropy to distinguish real secrets from benign strings
fn calculate_entropy(text: &str) -> f64 {
    let mut char_counts = std::collections::HashMap::new();
    for ch in text.chars() {
        *char_counts.entry(ch).or_insert(0) += 1;
    }

    let len = text.chars().count() as f64;
    let mut entropy = 0.0;

    for &count in char_counts.values() {
        let p = count as f64 / len;
        entropy -= p * p.log2();
    }

    entropy
}
```

### Security Patterns

Multi-severity patterns with entropy thresholds:

- **Critical**: `-----BEGIN PRIVATE KEY-----` (always flagged)
- **High**: Password/API key patterns with entropy > 4.0
- **Medium**: Generic secret patterns with entropy > 3.5
- **Low**: Debug credentials with entropy > 3.0

### File Exclusions

Skips common non-sensitive files:
- `package-lock.json`, `yarn.lock`, `Cargo.lock`, `Pipfile.lock`

## Performance & Caching

### TTL-Based Caching

5-minute cache with smart keys based on analysis parameters:

```rust
fn make_cache_key(path: &str, frameworks: bool, languages: bool, security: bool) -> String {
    format!("{}:f{}l{}s{}", path, frameworks as u8, languages as u8, security as u8)
}
```

### Git Optimization

Avoids O(n) commit walking for large repositories:
- `git rev-list --count HEAD` for commit counts
- `git rev-parse HEAD` for latest commit hash
- `git log --pretty=format:%an -100` for recent authors (limited sampling)

## API Usage Examples

### Framework Detection

```bash
# Detect frameworks in a directory
curl -X POST http://localhost:8080/api/v1/codegraph/frameworks/detect \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/path/to/project",
    "framework_types": ["React", "Django"]
  }'

# Response
{
  "frameworks": [
    {
      "name": "React",
      "version": "18.2.0",
      "confidence": 0.95,
      "rank": 9,
      "evidence": ["npm:react@18.2.0", "npm:react-dom@18.2.0"]
    }
  ],
  "timestamp": "2025-11-13T02:14:14.000Z",
  "analysis_time_ms": 45
}
```

### Repository Metadata Analysis

```bash
# Comprehensive repository analysis
curl -X POST http://localhost:8080/api/v1/codegraph/repository/metadata \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/path/to/repository",
    "include_frameworks": true,
    "include_languages": true,
    "include_security": true
  }'

# Response (cached for 5 minutes)
{
  "path": "/path/to/repository",
  "frameworks": [...],
  "languages": [
    {"name": "TypeScript", "files": 45, "lines": 12340, "percentage": 67.2}
  ],
  "security": {
    "violations": [
      {
        "file_path": "config/database.js",
        "pattern": "hardcoded_password",
        "line_number": 15,
        "severity": "high"
      }
    ],
    "scan_timestamp": "2025-11-13T02:14:14.000Z",
    "status": "completed"
  },
  "git_info": {
    "branch": "main",
    "commit_count": 1250,
    "last_commit": "a1b2c3d4...",
    "authors": ["Alice", "Bob", "Charlie"]
  },
  "timestamp": "2025-11-13T02:14:14.000Z",
  "analysis_time_ms": 125
}
```

## Performance Characteristics

- **Framework Detection**: ~50-200ms for typical projects (no caching)
- **Repository Metadata**: ~100-500ms with 5-minute TTL caching
- **Git Analysis**: O(1) operations, sub-millisecond response
- **Security Scanning**: ~10-50ms with entropy analysis
- **Memory Usage**: Bounded caching, automatic cleanup

## Integration Points

### With AdapterOS Router

Framework detection results provide routing metadata for the K-sparse LoRA router:

```rust
// Framework confidence scores influence adapter selection
let frameworks = detect_frameworks(repo_path)?;
for framework in frameworks {
    if framework.confidence > 0.8 {
        router.add_adapter_context(framework.name, framework.rank);
    }
}
```

### With Repository Management

Repository metadata integrates with the existing git repository handlers:

```rust
// Enhanced repository registration with framework detection
let metadata = get_repository_metadata(repo_path)?;
repository.set_frameworks(metadata.frameworks);
repository.set_languages(metadata.languages);
repository.set_security_status(metadata.security);
```

### With Training Pipeline

Framework and language analysis inform training data preparation:

```rust
// Select appropriate adapters based on detected frameworks
let frameworks = detect_frameworks(project_path)?;
let adapter_category = match frameworks.first() {
    Some(f) if f.name == "React" => "frontend-typescript",
    Some(f) if f.name == "Django" => "backend-python",
    _ => "general-code"
};
```

## Security Considerations

- **Path validation**: All paths validated using `adapteros_secure_fs`
- **Permission checks**: Directory access verified before analysis
- **Entropy analysis**: Distinguishes real secrets from benign strings
- **File exclusions**: Skips lockfiles and generated files
- **Rate limiting**: Inherits from tenant-level rate limits

## Error Handling

- **Framework detection failures**: Return specific error codes
- **Partial analysis**: Continues with available data when components fail
- **Cache misses**: Graceful fallback to fresh analysis
- **Git failures**: Returns minimal info rather than failing completely

## Future Extensions

- **Symbol analysis**: Tree-sitter integration for code intelligence
- **Test impact analysis**: Determine which tests to run for changed files
- **Call graph construction**: Map function dependencies
- **CAS storage**: Store analysis results in content-addressable storage
- **Incremental updates**: Update analysis results for partial changes
