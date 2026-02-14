# Phase 7: The Bootstrap Loop

## Problem

Phases 1-6 build the components. This phase closes the loop:

```
Codebase → Training Data → Adapter → Generate Code → Evaluate →
  ↑                                                      │
  └──────────────── Improve Codebase ←───────────────────┘
```

The system must be able to:
1. Train an adapter on its current codebase
2. Use that adapter to generate improvements
3. Validate improvements (compile + test)
4. Accept improvements that pass the gate
5. Retrain the adapter on the improved codebase
6. Repeat

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Bootstrap Controller                   │
│                                                          │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐            │
│  │ 1. Ingest│──>│ 2. Train │──>│ 3. Eval  │            │
│  │ Codebase │   │ Adapter  │   │ Quality  │            │
│  └──────────┘   └──────────┘   └────┬─────┘            │
│                                      │                   │
│                          ┌───────────┴───────────┐      │
│                          │                       │      │
│                     Pass │                  Fail │      │
│                          v                       v      │
│                   ┌──────────┐          ┌──────────┐   │
│                   │ 4.Promote│          │ Revert   │   │
│                   │ Adapter  │          │ to prev  │   │
│                   └────┬─────┘          └──────────┘   │
│                        │                                │
│                        v                                │
│                   ┌──────────┐                          │
│                   │ 5.Generate│                          │
│                   │ Proposals │                          │
│                   └────┬─────┘                          │
│                        │                                │
│                        v                                │
│                   ┌──────────┐                          │
│                   │ 6.Validate│                          │
│                   │ Proposals │                          │
│                   └────┬─────┘                          │
│                        │                                │
│                   ┌────┴────┐                           │
│              Pass │    Fail │                           │
│                   v         v                           │
│            ┌──────────┐  ┌──────────┐                  │
│            │ 7.Apply  │  │ Discard  │                  │
│            │ to branch│  │          │                  │
│            └────┬─────┘  └──────────┘                  │
│                 │                                       │
│                 v                                       │
│            ┌──────────┐                                │
│            │ 8.Retrain│──> Back to step 2              │
│            │ on new   │                                │
│            └──────────┘                                │
└─────────────────────────────────────────────────────────┘
```

## Implementation

### 1. Bootstrap Controller

```rust
pub struct BootstrapController {
    config: BootstrapConfig,
    ingestion: CodebaseIngestion,
    evaluator: CodeGenEvaluator,
    db: Db,
}

pub struct BootstrapConfig {
    /// Repository to self-train on
    pub repo_path: PathBuf,
    /// Base model for training
    pub base_model: String,
    /// Training configuration
    pub training: TrainingConfig,
    /// Minimum quality for promotion
    pub min_compile_rate: f32,
    pub min_test_pass_rate: f32,
    /// Maximum iterations before stopping
    pub max_iterations: u32,
    /// Branch to apply proposals to
    pub target_branch: String,
    /// Types of proposals to generate
    pub proposal_types: Vec<ProposalType>,
}
```

### 2. Proposal Types

```rust
pub enum ProposalType {
    /// Fill in TODO/unimplemented!() bodies
    FillTodo,
    /// Add missing documentation
    AddDocumentation,
    /// Implement suggested functions from comments
    ImplementSuggested,
    /// Improve error messages
    ImproveErrors,
    /// Add missing test cases
    AddTests,
    /// Refactor functions exceeding complexity threshold
    RefactorComplex,
}
```

### 3. Proposal Generator

```rust
pub struct ProposalGenerator {
    inference: InferencePipeline,
    rag: CodeContextRetriever,
}

impl ProposalGenerator {
    /// Generate code proposals for a given type.
    ///
    /// Scans the codebase for opportunities matching the proposal type,
    /// then uses the trained adapter + RAG to generate concrete code.
    pub async fn generate_proposals(
        &self,
        proposal_type: &ProposalType,
        adapter_id: &str,
        max_proposals: usize,
    ) -> Result<Vec<CodeProposal>> { ... }
}
```

### 4. Proposal Validator

```rust
pub struct ProposalValidator {
    checker: CompilationChecker,
    workspace_root: PathBuf,
}

impl ProposalValidator {
    /// Validate a code proposal.
    ///
    /// 1. Apply proposal to a git worktree
    /// 2. Run cargo check
    /// 3. Run cargo test for affected crate
    /// 4. Diff output to verify no regressions
    pub async fn validate(
        &self,
        proposal: &CodeProposal,
    ) -> ValidationResult { ... }
}
```

### 5. Bootstrap Iteration

```rust
impl BootstrapController {
    pub async fn run_iteration(&self) -> Result<IterationResult> {
        // 1. Ingest current codebase
        let ingestion_result = self.ingestion.ingest_and_train(
            &self.config.repo_path,
            &format!("self-v{}", iteration),
            &adapters_root,
        ).await?;

        // 2. Evaluate adapter quality
        let eval = self.evaluator.evaluate(&ingestion_result.adapter_id).await?;

        if !eval.passed_promotion_gate {
            return Ok(IterationResult::QualityGateFailed(eval));
        }

        // 3. Generate proposals
        let proposals = self.generator.generate_proposals(
            &self.config.proposal_types,
            &ingestion_result.adapter_id,
            50, // max proposals per iteration
        ).await?;

        // 4. Validate proposals
        let mut accepted = Vec::new();
        for proposal in &proposals {
            match self.validator.validate(proposal).await? {
                ValidationResult::Passed => accepted.push(proposal.clone()),
                ValidationResult::Failed(reason) => {
                    tracing::info!(reason, "Proposal rejected");
                }
            }
        }

        // 5. Apply accepted proposals
        if !accepted.is_empty() {
            self.apply_proposals(&accepted).await?;
        }

        Ok(IterationResult::Success {
            adapter_id: ingestion_result.adapter_id,
            proposals_generated: proposals.len(),
            proposals_accepted: accepted.len(),
            eval,
        })
    }
}
```

### 6. Safety Rails

- **Git worktree isolation**: All proposals are tested in a separate worktree
- **Maximum diff size**: No single proposal can modify more than 100 lines
- **Crate boundary**: Proposals can only modify one crate at a time
- **No dependency changes**: Proposals cannot modify Cargo.toml
- **Human review gate**: Option to require human approval before applying
- **Revert on regression**: If test suite regresses after applying, auto-revert

### 7. CLI Command

```bash
# Single iteration
./aosctl self-train --repo . --base-model qwen2.5-7b --iteration 1

# Continuous loop with max iterations
./aosctl self-train --repo . --max-iterations 10 --auto-apply

# Dry run (generate proposals but don't apply)
./aosctl self-train --repo . --dry-run

# Generate and review proposals interactively
./aosctl self-train --repo . --review
```

## Existing Code to Reuse

- `CodebaseIngestion` — full ingestion pipeline
- `MicroLoRATrainer` — training loop
- `InferencePipeline` — generation
- `CodeGraph` — codebase analysis
- `RagSystem` — context retrieval
- Git worktree support via `git2` crate (already a dependency)
- `adapteros-db` promotions — adapter lifecycle

## Tests

1. Single iteration produces valid proposals
2. Compilation check catches syntax errors in proposals
3. Test regression detection works
4. Git worktree isolation prevents main branch contamination
5. Maximum diff size limit is enforced
6. Revert on regression works correctly

## Hours: 200

- Bootstrap controller: 40h
- Proposal generator (6 types): 48h
- Proposal validator: 32h
- Safety rails: 24h
- CLI command: 16h
- Git worktree integration: 16h
- Tests: 24h
