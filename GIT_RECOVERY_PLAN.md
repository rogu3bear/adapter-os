# Git Recovery Plan

**Date**: 2025-01-15  
**Current Branch**: `staging/determinism-policy-validation`  
**Status**: Uncommitted changes + untracked files + branch cleanup needed

## Current State Analysis

### Modified Files (22 files, ~1137 additions, 219 deletions)
- **Git subsystem**: `branch_manager.rs`, `commit_daemon.rs`, `subsystem.rs`, `lib.rs`
- **Crypto/Keychain**: Major updates to `keychain.rs` (+523 lines)
- **Policy**: Determinism policy enhancements
- **API**: New handlers (`git_repository.rs`), error updates, route changes
- **Worker**: Inference pipeline and lib updates
- **System Metrics**: Database changes
- **Menu Bar App**: Status reader/view model/view updates
- **Tests**: New git repository integration tests

### Untracked Files (High Priority)
- **Documentation**: `LAUNCH_IMPROVEMENTS.md`, `MODEL_VALIDATION_PATCH.md`, `QA_ISSUES.md`, `RECTIFIED_REGISTRY_MIGRATION.md`, `SERVICE_STATUS.md`
- **New Features**: Circuit breaker, retry policies, progress service, stress test
- **Migrations**: Progress events, model metadata
- **Tools**: `aos`, `aos-launch`, `create_aos.rs`, `aos2_implementation.rs`
- **Config**: New config directory
- **UI Components**: Persona journey demo components

### Branch Status
- **Current**: `staging/determinism-policy-validation` (HEAD at merge commit)
- **Staging branches** (all pointing to same merge):
  - `staging/aos2-format`
  - `staging/domain-adapters-executor`
  - `staging/federation-daemon-integration`
  - `staging/keychain-integration`
  - `staging/repository-codegraph-integration`
  - `staging/streaming-api-integration`
  - `staging/system-metrics-postgres`
  - `staging/testing-infrastructure`
  - `staging/ui-backend-integration`
- **Auto-generated branches** (timestamp-based):
  - `2025-10-17-6skz-34460`
  - `2025-10-17-lkzg-d47a9`
  - `2025-10-29-4vzm-N1AHq`
  - `2025-10-29-5bph-ZQpnI`
  - `2025-10-29-62r2-cU3r9`
  - `2025-10-29-8roq-DLX4w`
  - `2025-10-29-mcpb-d1G9z`
  - `2025-10-29-mh8z-tw3Tz` (recently merged)
  - `2025-10-29-q62c-hD3ca`
- **Feature branch**: `feat-adapter-packaging-2c34c`
- **Main branch**: `main` (ahead with recent commits)

### Stashes
- `stash@{0}`: "Stash uncommitted changes before merge unification" (on main)
- `stash@{1}`: "WIP on auto/implement-domain-adapter-api-logic-uoqfu9"

## Recovery Steps

### Phase 1: Save Current Work

1. **Commit current changes** on `staging/determinism-policy-validation`:
   ```bash
   git add crates/adapteros-git/ crates/adapteros-crypto/ crates/adapteros-policy/ \
          crates/adapteros-server-api/ crates/adapteros-lora-worker/ \
          crates/adapteros-system-metrics/ menu-bar-app/ tests/integration/
   git commit -m "feat: git repository integration and determinism policy improvements

   - Enhanced git subsystem with branch manager and commit daemon updates
   - Major keychain provider improvements (+523 lines)
   - Added git repository handler and integration tests
   - Updated determinism policy enforcement
   - Improved inference pipeline and worker lib
   - Enhanced system metrics database integration
   - Updated menu bar app status components"
   ```

2. **Add and commit documentation**:
   ```bash
   git add *.md config/ docs/
   git commit -m "docs: add operational documentation and configuration

   - Launch improvements and readme
   - Model validation patch documentation
   - QA issues tracking
   - Registry migration documentation
   - Service status documentation"
   ```

3. **Add new features** (circuit breaker, retry, progress):
   ```bash
   git add crates/adapteros-core/src/circuit_breaker.rs \
          crates/adapteros-core/src/retry_*.rs \
          crates/adapteros-orchestrator/src/progress_service.rs \
          crates/adapteros-server-api/src/progress_service.rs \
          crates/adapteros-server-api/src/retry_metrics.rs \
          crates/adapteros-server-api/src/stress_test.rs \
          crates/adapteros-policy/src/packs/circuit_breaker.rs \
          docs/RETRY_POLICY_STANDARDIZATION.md
   git commit -m "feat: add circuit breaker, retry policies, and progress tracking

   - Circuit breaker implementation for resilience
   - Retry policies with metrics and strategies
   - Progress service for async operations
   - Stress test utilities"
   ```

4. **Add migrations and database updates**:
   ```bash
   git add migrations/ crates/adapteros-db/src/progress_events.rs \
          crates/adapteros-db/build.rs crates/adapteros-registry/migrations/
   git commit -m "feat: add database migrations for progress events and model metadata

   - Progress events tracking
   - Model metadata enhancements
   - Database build script updates"
   ```

5. **Add AOS format tools**:
   ```bash
   git add aos aos-launch create_aos.rs aos2_implementation.rs \
          aos_format_design.md Cargo.toml.aos
   git commit -m "feat: add AOS format tools and implementation

   - AOS CLI tool
   - AOS launch script
   - AOS2 format implementation
   - Format design documentation"
   ```

6. **Add UI components**:
   ```bash
   git add ui/src/components/Persona*.tsx ui/src/components/Stage*.tsx \
          ui/src/components/persona-stages/ ui/src/data/persona-journeys.ts \
          ui/src/pages/PersonasPage.tsx ui/src/config/
   git commit -m "feat(ui): add persona journey demo components

   - Interactive persona journey demo
   - Persona slider and stage viewers
   - Journey data and navigation utilities"
   ```

7. **Add remaining files** (scripts, tests, etc.):
   ```bash
   git add scripts/ tests/operational_resilience.rs \
          tests/sqlx_offline_validation.rs menu-bar-app/Tests/ \
          menu-bar-app/Sources/AdapterOSMenu/Services/*.swift \
          menu-bar-app/*.md launch.sh
   git commit -m "chore: add scripts, tests, and service management updates

   - Backup and SQLx setup scripts
   - Operational resilience tests
   - Menu bar app service improvements
   - Service management documentation"
   ```

### Phase 2: Branch Cleanup

1. **Identify branches to keep**:
   - Keep: `main`, current `staging/determinism-policy-validation`
   - Evaluate: Other staging branches (check if they have unique commits)
   - Delete: Auto-generated timestamp branches (already merged or obsolete)

2. **Check which staging branches have unique commits**:
   ```bash
   for branch in staging/*; do
     echo "=== $branch ==="
     git log --oneline main..$branch | head -5
   done
   ```

3. **Delete merged/obsolete branches**:
   ```bash
   # Delete auto-generated branches (already merged)
   git branch -d 2025-10-29-mh8z-tw3Tz  # Already merged
   git branch -D 2025-10-17-6skz-34460  # Check if safe to delete
   # ... (review each before deleting)
   ```

4. **Consolidate staging branches**:
   - If staging branches point to same commit, consider merging into main or deleting
   - Keep only branches with active development

### Phase 3: Stash Management

1. **Review stashes**:
   ```bash
   git stash show -p stash@{0}
   git stash show -p stash@{1}
   ```

2. **Apply or drop stashes** based on content relevance

### Phase 4: Documentation

1. **Update CHANGELOG.md** with all recent work
2. **Create branch documentation** explaining what each staging branch is for
3. **Document migration path** for future reference

## Risks and Testing

### Risks
1. **Data Loss**: Accidental deletion of uncommitted work
   - **Mitigation**: Create backup commits before cleanup
   - **Test**: Verify all changes are committed before branch deletion

2. **Branch Confusion**: Deleting branches with unique work
   - **Mitigation**: Check for unique commits before deletion
   - **Test**: Compare each branch with main before deletion

3. **Merge Conflicts**: Future merges may conflict
   - **Mitigation**: Keep staging branches until main is updated
   - **Test**: Attempt merge to main before cleanup

### Testing Checklist
- [ ] All modified files committed
- [ ] All untracked files added and committed
- [ ] No unstaged changes remain
- [ ] Each branch checked for unique commits
- [ ] Stashes reviewed and handled
- [ ] Build succeeds after cleanup
- [ ] Tests pass

## Next Steps After Recovery

1. **Create feature branches** for new work instead of staging branches
2. **Commit frequently** with descriptive messages
3. **Document branches** in README or BRANCHES.md
4. **Regular cleanup** of merged branches
5. **Use conventional commits** format for consistency

