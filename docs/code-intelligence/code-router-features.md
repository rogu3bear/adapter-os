# Code Router Features

## Overview

The router for code intelligence tasks requires specialized features beyond generic adapter selection. These features capture language, framework, symbol context, and task intent to make informed K-sparse decisions.

## Feature Vector

Each inference request computes a **feature vector** fed into the router's scoring function:

```rust
pub struct CodeFeatures {
    // Language detection (one-hot or embedding)
    pub lang_one_hot: Vec<f32>,        // [0,0,1,0,0,...] for TypeScript
    
    // Framework signals
    pub framework_prior: Vec<f32>,     // Prior probabilities per framework
    
    // Symbol context
    pub symbol_hits: f32,              // Count of matched symbols (0-N)
    pub symbol_confidence: f32,        // Average confidence of hits (0-1)
    
    // Path context
    pub path_tokens: Vec<f32>,         // Hashed path segments (bloom filter)
    
    // Attention entropy
    pub attn_entropy: f32,             // Rolling entropy from recent tokens
    
    // Commit context
    pub commit_hint: f32,              // 1.0 if ephemeral adapter exists, else 0.0
    
    // Task intent
    pub prompt_verb: Vec<f32>,         // One-hot for explain/generate/refactor/test
    
    // Retrieval context
    pub retrieval_quality: f32,        // Quality of RAG evidence (0-1)
    pub retrieval_count: f32,          // Number of retrieved chunks
}
```

## Feature Extraction

### 1. Language One-Hot (`lang_one_hot`)

**Purpose**: Detect programming language from file extension or code fence.

**Extraction**:
```rust
pub fn extract_lang_one_hot(prompt: &str, context_file: Option<&Path>) -> Vec<f32> {
    let lang = detect_language(prompt, context_file);
    let mut one_hot = vec![0.0; NUM_LANGUAGES];
    one_hot[lang as usize] = 1.0;
    one_hot
}

fn detect_language(prompt: &str, context_file: Option<&Path>) -> Language {
    // Priority 1: File extension
    if let Some(file) = context_file {
        if let Some(ext) = file.extension() {
            return match ext.to_str() {
                Some("py") => Language::Python,
                Some("rs") => Language::Rust,
                Some("ts") | Some("tsx") => Language::TypeScript,
                Some("js") | Some("jsx") => Language::JavaScript,
                Some("go") => Language::Go,
                Some("java") => Language::Java,
                _ => Language::Unknown,
            };
        }
    }
    
    // Priority 2: Code fence in prompt
    if let Some(fence_lang) = extract_code_fence_lang(prompt) {
        return fence_lang;
    }
    
    // Priority 3: Heuristics (keywords, syntax)
    detect_from_keywords(prompt)
}
```

**Dimensionality**: ~10-20 (number of supported languages)

---

### 2. Framework Prior (`framework_prior`)

**Purpose**: Boost framework adapters based on detected frameworks in the repo.

**Extraction**:
```rust
pub fn extract_framework_prior(
    repo_id: &str,
    context_file: Option<&Path>,
    registry: &Registry,
) -> Result<Vec<f32>> {
    // Load frameworks.json for this repo
    let frameworks = registry.get_repo_frameworks(repo_id)?;
    
    let mut priors = vec![0.0; NUM_FRAMEWORKS];
    
    for framework in frameworks {
        let idx = framework_to_index(&framework.name);
        priors[idx] = 1.0;
        
        // Boost if context file is in framework-specific directory
        if let Some(file) = context_file {
            if is_framework_related(file, &framework) {
                priors[idx] = 2.0;  // Double boost
            }
        }
    }
    
    Ok(priors)
}

fn is_framework_related(file: &Path, framework: &Framework) -> bool {
    match framework.name.as_str() {
        "django" => file.starts_with("backend") || file.to_string_lossy().contains("django"),
        "react" => file.starts_with("frontend") || file.extension() == Some("tsx"),
        "pytest" => file.starts_with("tests") || file.file_name().unwrap().to_string_lossy().starts_with("test_"),
        // ...
        _ => false,
    }
}
```

**Dimensionality**: ~20-50 (number of supported frameworks)

---

### 3. Symbol Hits (`symbol_hits`, `symbol_confidence`)

**Purpose**: Count and quality of symbol matches from the index.

**Extraction**:
```rust
pub fn extract_symbol_features(
    prompt: &str,
    context_file: Option<&Path>,
    symbol_index: &SymbolIndex,
) -> Result<(f32, f32)> {
    // Extract candidate symbols from prompt
    let candidates = extract_symbol_candidates(prompt);
    
    let mut hit_count = 0.0;
    let mut total_confidence = 0.0;
    
    for candidate in candidates {
        if let Some(matches) = symbol_index.search(&candidate, context_file)? {
            for m in matches {
                hit_count += 1.0;
                total_confidence += m.confidence;
            }
        }
    }
    
    let avg_confidence = if hit_count > 0.0 {
        total_confidence / hit_count
    } else {
        0.0
    };
    
    Ok((hit_count, avg_confidence))
}

fn extract_symbol_candidates(prompt: &str) -> Vec<String> {
    // Tokenize and filter for symbol-like patterns
    let tokens = tokenize(prompt);
    tokens
        .into_iter()
        .filter(|t| is_symbol_like(t))
        .collect()
}

fn is_symbol_like(token: &str) -> bool {
    // CamelCase or snake_case, length 3+
    token.len() >= 3 &&
    (token.contains('_') || token.chars().any(|c| c.is_uppercase()))
}
```

**Dimensionality**: 2 scalars (hit count, average confidence)

---

### 4. Path Tokens (`path_tokens`)

**Purpose**: Encode file path structure to route to repo-specific adapters.

**Extraction**:
```rust
pub fn extract_path_tokens(context_file: Option<&Path>) -> Vec<f32> {
    const BLOOM_SIZE: usize = 256;
    let mut bloom = vec![0.0; BLOOM_SIZE];
    
    if let Some(file) = context_file {
        for component in file.components() {
            if let Some(comp_str) = component.as_os_str().to_str() {
                // Hash each path component into bloom filter
                let h1 = hash_to_index(comp_str, 0, BLOOM_SIZE);
                let h2 = hash_to_index(comp_str, 1, BLOOM_SIZE);
                let h3 = hash_to_index(comp_str, 2, BLOOM_SIZE);
                
                bloom[h1] = 1.0;
                bloom[h2] = 1.0;
                bloom[h3] = 1.0;
            }
        }
    }
    
    bloom
}

fn hash_to_index(s: &str, seed: u64, size: usize) -> usize {
    let hash = blake3::hash(format!("{}{}", seed, s).as_bytes());
    u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap()) as usize % size
}
```

**Dimensionality**: 256 (bloom filter)

---

### 5. Attention Entropy (`attn_entropy`)

**Purpose**: Detect model uncertainty; high entropy with low evidence → abstain.

**Extraction**:
```rust
pub fn extract_attn_entropy(recent_logits: &[Vec<f32>]) -> f32 {
    // Compute average entropy over last N tokens
    let window = 8;
    let relevant = &recent_logits[recent_logits.len().saturating_sub(window)..];
    
    let entropies: Vec<f32> = relevant
        .iter()
        .map(|logits| {
            let probs = softmax(logits);
            compute_entropy(&probs)
        })
        .collect();
    
    entropies.iter().sum::<f32>() / entropies.len() as f32
}

fn compute_entropy(probs: &[f32]) -> f32 {
    probs
        .iter()
        .filter(|&&p| p > 1e-9)
        .map(|&p| -p * p.log2())
        .sum()
}
```

**Dimensionality**: 1 scalar

---

### 6. Commit Hint (`commit_hint`)

**Purpose**: Bias toward ephemeral adapter if it exists for the current commit.

**Extraction**:
```rust
pub fn extract_commit_hint(
    repo_id: &str,
    commit_sha: Option<&str>,
    registry: &Registry,
) -> Result<f32> {
    if let Some(sha) = commit_sha {
        let ephemeral_exists = registry.has_ephemeral_for_commit(repo_id, sha)?;
        Ok(if ephemeral_exists { 1.0 } else { 0.0 })
    } else {
        Ok(0.0)
    }
}
```

**Dimensionality**: 1 scalar

---

### 7. Prompt Verb (`prompt_verb`)

**Purpose**: Detect task intent to route appropriately.

**Extraction**:
```rust
pub fn extract_prompt_verb(prompt: &str) -> Vec<f32> {
    let verbs = ["explain", "generate", "refactor", "test", "fix", "implement"];
    let mut one_hot = vec![0.0; verbs.len()];
    
    let prompt_lower = prompt.to_lowercase();
    for (i, &verb) in verbs.iter().enumerate() {
        if prompt_lower.contains(verb) {
            one_hot[i] = 1.0;
        }
    }
    
    // Normalize if multiple verbs detected
    let sum: f32 = one_hot.iter().sum();
    if sum > 1.0 {
        for v in &mut one_hot {
            *v /= sum;
        }
    }
    
    one_hot
}
```

**Dimensionality**: ~6-10 (number of task verbs)

---

### 8. Retrieval Quality (`retrieval_quality`, `retrieval_count`)

**Purpose**: Evidence strength influences routing and abstention.

**Extraction**:
```rust
pub fn extract_retrieval_features(evidence: &[EvidenceSpan]) -> (f32, f32) {
    let count = evidence.len() as f32;
    
    let quality = if count > 0.0 {
        let avg_score: f32 = evidence.iter().map(|e| e.score).sum::<f32>() / count;
        avg_score
    } else {
        0.0
    };
    
    (quality, count)
}
```

**Dimensionality**: 2 scalars

---

## Routing Logic

### Scoring Function

```rust
pub fn score_adapters(
    features: &CodeFeatures,
    adapters: &[AdapterMetadata],
) -> Vec<f32> {
    let mut scores = Vec::with_capacity(adapters.len());
    
    for adapter in adapters {
        let score = match adapter.category {
            AdapterCategory::Code => {
                // Always relevant for code tasks
                1.0 + features.symbol_hits * 0.1
            }
            AdapterCategory::Framework => {
                // Framework prior dominates
                let idx = framework_to_index(&adapter.framework_id.unwrap());
                features.framework_prior[idx]
            }
            AdapterCategory::Codebase => {
                // Symbol hits + path match
                let path_match = compute_path_match(&features.path_tokens, &adapter.repo_id.unwrap());
                features.symbol_hits * 0.3 + path_match * 0.7
            }
            AdapterCategory::Ephemeral => {
                // Commit hint + recency
                features.commit_hint * 2.0
            }
        };
        
        scores.push(score);
    }
    
    scores
}
```

### Top-K Selection with Constraints

```rust
pub fn route_with_constraints(
    scores: Vec<f32>,
    adapters: &[AdapterMetadata],
    k: usize,
) -> Decision {
    // Sort by score descending
    let mut indexed: Vec<(usize, f32)> = scores.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    
    // Apply constraints
    let mut selected = Vec::new();
    let mut framework_count = 0;
    
    for (idx, score) in indexed {
        if selected.len() >= k {
            break;
        }
        
        let adapter = &adapters[idx];
        
        // Max 1 framework adapter
        if adapter.category == AdapterCategory::Framework {
            if framework_count >= 1 {
                continue;
            }
            framework_count += 1;
        }
        
        selected.push((idx, score));
    }
    
    // Softmax with temperature and entropy floor
    let gates = compute_gates(&selected, 1.0, 0.02);
    
    Decision {
        indices: selected.iter().map(|(i, _)| *i as u16).collect(),
        gates_q15: quantize_q15(&gates),
    }
}
```

---

## Example Routing Decisions

### Scenario 1: Explain Function

**Input**:
- Prompt: "Explain how `process_payment` works"
- File: `src/payments/processor.py`
- Repo: `acme/payments`

**Features**:
- `lang_one_hot`: Python
- `framework_prior`: Django (2.0), FastAPI (0.0), ...
- `symbol_hits`: 3.0 (found `process_payment`, `Payment`, `Transaction`)
- `path_tokens`: [hashed "src", "payments", "processor"]
- `commit_hint`: 0.0

**Routing Decision** (K=3):
1. `codebase_acme_payments_v7` (score: 2.8)
2. `code_lang_v1` (score: 1.3)
3. `framework_django_v1` (score: 2.0)

---

### Scenario 2: Fix Failing Test (with Ephemeral)

**Input**:
- Prompt: "Fix the failing test in test_processor.py"
- File: `tests/test_processor.py`
- Repo: `acme/payments`
- Commit: `ab12cd34`

**Features**:
- `lang_one_hot`: Python
- `framework_prior`: pytest (2.0)
- `symbol_hits`: 2.0
- `commit_hint`: 1.0 (ephemeral exists)
- `prompt_verb`: "fix" (1.0)

**Routing Decision** (K=3):
1. `commit_ab12cd34` (score: 3.2)
2. `codebase_acme_payments_v7` (score: 2.5)
3. `framework_pytest_v1` (score: 2.0)

---

### Scenario 3: Generate React Component

**Input**:
- Prompt: "Generate a React form component with validation"
- File: `frontend/components/Form.tsx`
- Repo: `myapp`

**Features**:
- `lang_one_hot`: TypeScript
- `framework_prior`: React (2.0), Next.js (0.5)
- `symbol_hits`: 0.0 (new file)
- `prompt_verb`: "generate" (1.0)

**Routing Decision** (K=3):
1. `framework_react_v2` (score: 3.0)
2. `code_lang_v1` (score: 1.5)
3. `codebase_myapp_v3` (score: 0.8)

---

## Abstention Rules

Refuse when:
1. `symbol_hits == 0 && retrieval_count == 0` (no evidence)
2. `attn_entropy > 0.7 && retrieval_quality < 0.3` (uncertain + weak evidence)
3. No adapters score above threshold (0.5)

Return structured refusal:
```json
{
  "status": "insufficient_evidence",
  "needed": ["file_path", "symbol", "test_target"],
  "hint": "Provide file path or symbol name for better context"
}
```

---

## Performance Targets

- Feature extraction: <5ms
- Scoring: <3ms (100 adapters)
- Top-K selection: <2ms
- Total routing overhead: <10ms per token

---

## Configuration

Enable/disable features in manifest:

```yaml
router:
  code_features:
    enable_lang_detection: true
    enable_framework_prior: true
    enable_symbol_hits: true
    enable_path_tokens: true
    enable_commit_hint: true
    max_framework_adapters: 1
```
