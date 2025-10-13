# CodeGraph Specification

## Overview

The **CodeGraph** is a language-aware, deterministic representation of a repository's structure, symbols, and relationships. It serves as the foundation for symbol indexing, test mapping, framework detection, and evidence retrieval in code intelligence tasks.

## Goals

1. **Language-aware**: Parse syntax correctly using tree-sitter
2. **Deterministic**: Same repo + commit → same graph hash (BLAKE3)
3. **Complete**: Capture files, symbols, dependencies, tests
4. **Queryable**: Enable fast symbol lookup, call graph traversal, test impact analysis
5. **Portable**: Serialize to binary format, store in CAS

## Graph Schema

### Nodes

#### FileNode
```rust
pub struct FileNode {
    pub id: FileId,              // b3(repo_id || path)
    pub path: PathBuf,           // Relative to repo root
    pub language: Language,      // Python, Rust, TypeScript, etc.
    pub hash: B3Hash,            // Content hash
    pub size_bytes: u64,
    pub metadata: FileMetadata,
}

pub struct FileMetadata {
    pub is_test: bool,
    pub is_generated: bool,
    pub framework_hints: Vec<String>, // e.g., ["django", "pytest"]
}
```

#### SymbolNode
```rust
pub struct SymbolNode {
    pub id: SymbolId,            // b3(file_id || span || name)
    pub name: String,            // Function/class/variable name
    pub kind: SymbolKind,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub span: Span,              // Start/end line+col
    pub visibility: Visibility,  // Public, private, internal
}

pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Variable,
    Constant,
    Module,
    Route,            // HTTP endpoint
    GraphQLField,
    GrpcMethod,
    SQLTable,
}

pub struct Span {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}
```

#### TestNode
```rust
pub struct TestNode {
    pub id: TestId,
    pub name: String,
    pub kind: TestKind,          // Unit, integration, e2e
    pub target_symbols: Vec<SymbolId>, // What it tests
    pub span: Span,
}
```

#### FrameworkNode
```rust
pub struct FrameworkNode {
    pub id: FrameworkId,
    pub name: String,            // e.g., "django", "react"
    pub version: Option<String>,
    pub config_files: Vec<FileId>,
}
```

### Edges

```rust
pub enum Edge {
    Defines {
        from: FileId,
        to: SymbolId,
    },
    Calls {
        from: SymbolId,
        to: SymbolId,
        span: Span,            // Where the call happens
    },
    Imports {
        from: FileId,
        to: FileId,            // Or external module
        symbols: Vec<String>,  // Specific imports
    },
    TestCovers {
        from: TestId,
        to: SymbolId,
    },
    Inherits {
        from: SymbolId,        // Subclass/implementor
        to: SymbolId,          // Superclass/interface
    },
    FrameworkBinding {
        from: SymbolId,        // Route handler, component
        to: FrameworkId,
    },
    Owner {
        from: FileId,
        to: String,            // CODEOWNERS entry
    },
}
```

## Tree-sitter Integration

### Parser Setup

Each language requires a tree-sitter grammar:

```rust
use tree_sitter::{Parser, Language};

pub struct LanguageParser {
    parser: Parser,
    language: Language,
    queries: QuerySet,
}

impl LanguageParser {
    pub fn new(lang: LanguageConfig) -> Result<Self> {
        let mut parser = Parser::new();
        let language = match lang {
            LanguageConfig::Python => tree_sitter_python::language(),
            LanguageConfig::Rust => tree_sitter_rust::language(),
            LanguageConfig::TypeScript => tree_sitter_typescript::language_typescript(),
            // ...
        };
        parser.set_language(language)?;
        
        let queries = QuerySet::load_for_language(&lang)?;
        Ok(Self { parser, language, queries })
    }
}
```

### Query Patterns

Each language has **tree-sitter queries** to extract symbols:

**Python example** (`queries/python/symbols.scm`):
```scheme
; Functions
(function_definition
  name: (identifier) @function.name
  parameters: (parameters) @function.params
  body: (block) @function.body) @function.def

; Classes
(class_definition
  name: (identifier) @class.name
  superclasses: (argument_list)? @class.bases
  body: (block) @class.body) @class.def

; Django routes
(call
  function: (attribute
    object: (identifier) @_obj
    attribute: (identifier) @_method)
  arguments: (argument_list
    (string) @route.pattern)
  (#eq? @_obj "path")
  (#eq? @_method "path")) @route.def
```

**Rust example** (`queries/rust/symbols.scm`):
```scheme
; Functions
(function_item
  name: (identifier) @function.name
  parameters: (parameters) @function.params) @function.def

; Structs
(struct_item
  name: (type_identifier) @struct.name
  body: (field_declaration_list)? @struct.fields) @struct.def

; Traits
(trait_item
  name: (type_identifier) @trait.name
  body: (declaration_list) @trait.body) @trait.def
```

### Extraction Pipeline

```rust
pub fn extract_symbols(file: &FileNode, source: &str) -> Result<Vec<SymbolNode>> {
    let parser = LanguageParser::new(file.language.config())?;
    let tree = parser.parse(source)?;
    
    let mut symbols = Vec::new();
    let matches = parser.query_symbols(&tree, source)?;
    
    for m in matches {
        let symbol = SymbolNode {
            id: generate_symbol_id(&file.id, &m.span, &m.name),
            name: m.name,
            kind: m.kind,
            signature: extract_signature(&m, source),
            docstring: extract_docstring(&m, source),
            span: m.span,
            visibility: infer_visibility(&m),
        };
        symbols.push(symbol);
    }
    
    Ok(symbols)
}
```

## Framework Detection

### Detection Rules

Framework presence is determined by **fingerprint files**:

```rust
pub struct FrameworkFingerprint {
    pub framework: String,
    pub version_extract: VersionExtract,
    pub required_files: Vec<PathBuf>,
    pub optional_files: Vec<PathBuf>,
}

pub static FINGERPRINTS: &[FrameworkFingerprint] = &[
    FrameworkFingerprint {
        framework: "django".to_string(),
        version_extract: VersionExtract::FromFile("requirements.txt", r"Django==(\d+\.\d+)"),
        required_files: vec!["manage.py", "settings.py"],
        optional_files: vec!["urls.py", "wsgi.py"],
    },
    FrameworkFingerprint {
        framework: "react".to_string(),
        version_extract: VersionExtract::FromFile("package.json", r#""react":\s*"(\^?\d+\.\d+)"#),
        required_files: vec!["package.json"],
        optional_files: vec!["tsconfig.json", "next.config.js"],
    },
    // ...
];
```

### Detection Process

```rust
pub fn detect_frameworks(repo_path: &Path) -> Result<Vec<FrameworkNode>> {
    let mut detected = Vec::new();
    
    for fingerprint in FINGERPRINTS {
        if fingerprint.matches(repo_path)? {
            let version = fingerprint.extract_version(repo_path)?;
            let config_files = fingerprint.collect_config_files(repo_path)?;
            
            detected.push(FrameworkNode {
                id: generate_framework_id(&fingerprint.framework),
                name: fingerprint.framework.clone(),
                version,
                config_files,
            });
        }
    }
    
    Ok(detected)
}
```

## Test Mapping

### Discovery

Tests are identified by:
1. File naming patterns: `test_*.py`, `*_test.rs`, `*.test.ts`
2. Directory structure: `tests/`, `__tests__/`, `spec/`
3. Framework markers: `@pytest.mark`, `#[test]`, `describe()`

### Impact Analysis

Given a set of changed files, compute which tests should run:

```rust
pub fn compute_test_impact(
    graph: &CodeGraph,
    changed_files: &[FileId],
) -> Result<Vec<TestId>> {
    let mut impacted = HashSet::new();
    
    for file_id in changed_files {
        // Direct tests
        let direct_tests = graph.find_tests_for_file(file_id);
        impacted.extend(direct_tests);
        
        // Symbols in changed file
        let symbols = graph.symbols_in_file(file_id);
        for symbol in symbols {
            // Tests that directly cover this symbol
            let covering_tests = graph.tests_covering(symbol);
            impacted.extend(covering_tests);
            
            // Callers of this symbol (1-hop)
            let callers = graph.callers_of(symbol);
            for caller in callers {
                let caller_tests = graph.tests_covering(caller);
                impacted.extend(caller_tests);
            }
        }
    }
    
    Ok(impacted.into_iter().collect())
}
```

## Serialization

### Binary Format

CodeGraphs are serialized to a compact binary format for CAS storage:

```rust
pub struct CodeGraphBinary {
    pub version: u32,              // Schema version
    pub repo_id: String,
    pub commit_sha: String,
    pub timestamp: u64,
    pub nodes: NodeTable,
    pub edges: EdgeTable,
    pub indices: IndexTable,       // Precomputed indices
    pub hash: B3Hash,              // Self-hash
}

impl CodeGraphBinary {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        
        // Write header
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(self.repo_id.as_bytes());
        buf.extend_from_slice(&self.commit_sha.as_bytes());
        
        // Write nodes (length-prefixed)
        self.nodes.serialize_into(&mut buf)?;
        
        // Write edges
        self.edges.serialize_into(&mut buf)?;
        
        // Write indices
        self.indices.serialize_into(&mut buf)?;
        
        // Compute and append hash
        let hash = blake3::hash(&buf);
        buf.extend_from_slice(hash.as_bytes());
        
        Ok(buf)
    }
}
```

### Storage in CAS

```rust
pub async fn store_codegraph(
    cas: &CasStore,
    graph: &CodeGraph,
) -> Result<B3Hash> {
    let binary = graph.to_binary()?;
    let serialized = binary.serialize()?;
    let hash = cas.put(&serialized).await?;
    
    // Metadata sidecar
    let metadata = CodeGraphMetadata {
        repo_id: graph.repo_id.clone(),
        commit_sha: graph.commit_sha.clone(),
        file_count: graph.files.len(),
        symbol_count: graph.symbols.len(),
        languages: graph.languages(),
        frameworks: graph.frameworks.iter().map(|f| f.name.clone()).collect(),
    };
    cas.put_metadata(&hash, &metadata).await?;
    
    Ok(hash)
}
```

## Determinism

### Stable Hashing

All IDs are deterministically derived:

```rust
pub fn generate_file_id(repo_id: &str, path: &Path) -> FileId {
    let input = format!("{}:{}", repo_id, path.display());
    FileId(blake3::hash(input.as_bytes()).into())
}

pub fn generate_symbol_id(file_id: &FileId, span: &Span, name: &str) -> SymbolId {
    let input = format!("{}:{}:{}:{}:{}", 
        file_id.0, span.start_line, span.start_col, span.end_line, name);
    SymbolId(blake3::hash(input.as_bytes()).into())
}
```

### Ordering

All collections are sorted before serialization:
- Files by path (lexicographic)
- Symbols by (file_id, start_line, start_col, name)
- Edges by (from_id, edge_type, to_id)

### Reproducibility

Given identical:
- Repository state (same commit)
- Tree-sitter version
- Parser grammar version
- Extraction queries

The CodeGraph hash will be **byte-identical** across builds.

## Usage Examples

### Build a CodeGraph

```rust
use aos_codegraph::{CodeGraphBuilder, LanguageConfig};

let builder = CodeGraphBuilder::new("my-repo", "abc123");

// Scan directory
builder.scan_directory("src/", &[
    LanguageConfig::Python,
    LanguageConfig::Rust,
])?;

// Detect frameworks
builder.detect_frameworks()?;

// Build call graph
builder.build_call_graph()?;

// Map tests
builder.map_tests()?;

let graph = builder.build()?;
let hash = graph.hash();
```

### Query Symbols

```rust
// Find all functions in a file
let functions = graph.symbols_in_file(&file_id)
    .filter(|s| s.kind == SymbolKind::Function)
    .collect::<Vec<_>>();

// Find callers of a function
let callers = graph.callers_of(&symbol_id);

// Find tests covering a symbol
let tests = graph.tests_covering(&symbol_id);
```

### Impact Analysis

```rust
let changed_files = vec![file_id_1, file_id_2];
let impacted_tests = graph.compute_test_impact(&changed_files)?;

println!("Run {} tests", impacted_tests.len());
```

## Integration with aos-artifacts

CodeGraphs are stored as CAS artifacts:

```bash
# During scan
codegraph_hash=$(aos-codegraph build --repo $REPO --commit $SHA)

# Store in CAS
aosctl import --type codegraph --hash $codegraph_hash --file codegraph.bin

# Retrieve later
aosctl export --hash $codegraph_hash --output codegraph.bin
```

## Performance Targets

- **Parse time**: <5s per 10K LOC (Python)
- **Graph build**: <10s for medium repo (50K LOC)
- **Symbol lookup**: <10ms (indexed)
- **Test impact analysis**: <500ms (precomputed)
- **Serialization**: <1s
- **Storage**: ~1-5 MB per 10K LOC (compressed)

## Dependencies

Required crates:
- `tree-sitter` ^0.20
- `tree-sitter-python` ^0.20
- `tree-sitter-rust` ^0.20
- `tree-sitter-typescript` ^0.20
- `tree-sitter-go` ^0.19
- `tree-sitter-java` ^0.19
- `blake3` ^1.5
- `serde` ^1.0
- `bincode` ^1.3

See [code-dependencies.md](code-dependencies.md) for full list.
