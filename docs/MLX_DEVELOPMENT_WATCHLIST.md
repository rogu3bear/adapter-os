# MLX Development Watchlist

**Purpose:** Monitor MLX framework development for C++ API updates, production-readiness signals, and features relevant to AdapterOS integration
**Monitoring Frequency:** Weekly review recommended
**Last Updated:** 2025-11-19
**Maintained by:** Agent 11 - MLX Path Planner

---

## 1. Critical Repositories

### 1.1 Primary MLX Repository
**URL:** https://github.com/ml-explore/mlx
**Watch:** Releases, Pull Requests, Issues

**Key Areas:**
- [ ] **C++ API Changes:** Monitor `mlx/cpp/` directory
- [ ] **Hidden State Extraction:** Search for "forward hooks" or "intermediate activations"
- [ ] **LoRA Improvements:** Track LoRA training updates
- [ ] **Memory Management:** Watch unified memory optimizations
- [ ] **Quantization:** Track 4-bit/8-bit precision improvements

**Notification Setup:**
```bash
# GitHub CLI (gh) watch setup
gh repo view ml-explore/mlx --watch
gh api -X PUT /repos/ml-explore/mlx/subscription \
  -f subscribed=true -f ignored=false
```

**Critical Issues to Monitor:**
- Issues tagged `C++`, `API`, `LoRA`, `memory`
- PRs modifying `mlx/cpp/` or `mlx/core/`
- Any discussion of "hidden states" or "activation checkpointing"

### 1.2 MLX C API Repository
**URL:** https://github.com/ml-explore/mlx-c
**Watch:** All activity (smaller repo)

**Key Areas:**
- [ ] **Production Readiness:** Look for "stable", "1.0", "production" tags
- [ ] **Feature Parity:** Compare C API vs C++ API completeness
- [ ] **Documentation Updates:** Track mlx-c API docs
- [ ] **Example Code:** New examples show recommended patterns

**Status Indicators:**
- ✅ **Production Ready:** Version 1.0 release, stable tag
- ⚠️ **Beta:** Active development, frequent breaking changes
- ❌ **Alpha:** Experimental, no stability guarantees

**Current Status (2025-11-19):** Beta (131 commits, 150 stars, MIT license)

### 1.3 MLX Swift Repository
**URL:** https://github.com/ml-explore/mlx-swift
**Watch:** Releases (indicates mlx-c maturity)

**Relevance:** Swift bindings use mlx-c as bridge, similar to our approach
- If Swift is stable → mlx-c is likely production-ready
- Swift issues may reveal mlx-c limitations
- Swift examples show best practices for FFI

### 1.4 FastMLX Repository
**URL:** https://github.com/Blaizzy/fastmlx
**Watch:** Releases, Issues

**Relevance:** Production-ready MLX API hosting
- Monitor deployment patterns
- Error handling strategies
- Performance optimizations
- Production issues (crashes, memory leaks)

---

## 2. Version Compatibility Matrix

### 2.1 Current Versions (2025-11-19)

| Component | Version | Release Date | Status |
|-----------|---------|--------------|--------|
| MLX (main) | 0.29.5 | 2025-11-11 | Stable |
| mlx-c | N/A (dev) | 2025-11-?? | Beta |
| MLX Swift | N/A | 2025-11-?? | Active |
| FastMLX | Latest | 2025-11-?? | Production |

### 2.2 Compatibility Tracking

**Tested Configurations:**
```toml
# Cargo.toml dependency tracking (future)
[dependencies.mlx-sys]
version = "0.29"  # Pin to specific minor version
features = ["cpp-api"]

# OR

[build-dependencies]
mlx-cpp = { version = "0.29", optional = true }
```

**Breaking Changes Log:**
- [ ] MLX 0.30: TBD
- [ ] MLX 0.29: Last tested version
- [x] MLX 0.28: Initial evaluation

**Update Strategy:**
1. Test new MLX versions in isolated branch
2. Run full test suite before merging
3. Update docs with compatibility notes
4. Pin dependencies after validation

---

## 3. Feature Request Tracking

### 3.1 Critical Features for AdapterOS

#### FR-1: Hidden State Extraction API
**Status:** ⚠️ Needs Verification
**Priority:** P0 (blocker)

**Description:**
Expose intermediate activations from transformer layers for LoRA application

**MLX Requirements:**
```cpp
// Desired API
std::unordered_map<std::string, mlx::core::array>
mlx::nn::Module::forward_with_hooks(
    const mlx::core::array& input,
    const std::vector<std::string>& hook_names
);
```

**Monitoring:**
- Search MLX issues for: "forward hooks", "intermediate activations", "layer outputs"
- Check MLX Python API for `register_forward_hook()` equivalent
- Monitor PRs touching `mlx/nn/` directory

**Workaround if not available:**
- Fork MLX and add custom hooks
- Modify model definition to return intermediate values
- Use graph inspection API (if available)

#### FR-2: Multi-Adapter LoRA Routing
**Status:** ⚠️ Needs Implementation
**Priority:** P1 (important)

**Description:**
Apply K-sparse routing with multiple LoRA adapters efficiently

**MLX Requirements:**
```cpp
// Desired API
mlx::core::array mlx::lora::multi_forward(
    const mlx::core::array& input,
    const std::vector<LoRAAdapter>& adapters,
    const std::vector<float>& gates
);
```

**Monitoring:**
- Track MLX LoRA module updates
- Search for "multi-adapter", "router", "mixture of experts"
- Monitor `mlx/lora/` directory

**Current Approach:**
- Implement in AdapterOS layer (not MLX)
- Use MLX primitives (matmul, add) for routing

#### FR-3: Deterministic Execution Mode
**Status:** ❌ Not Available
**Priority:** P2 (nice-to-have)

**Description:**
Reproducible inference with seeded randomness

**MLX Requirements:**
```cpp
mlx::core::random::seed(uint64_t seed);
mlx::core::set_deterministic_mode(true);
```

**Monitoring:**
- Search for "deterministic", "reproducible", "seed"
- Track Metal shader updates (source of non-determinism)
- Monitor quantization PRs

**Status Notes:**
- MLX may never be fully deterministic (Metal GPU limitations)
- AdapterOS policy: MLX backend = experimental (non-deterministic OK)

### 3.2 Feature Request Template

**When to file MLX feature request:**
1. Feature blocks AdapterOS integration
2. Workaround is too complex or slow
3. Feature benefits broader MLX community

**Template:**
```markdown
**Title:** [Feature Request] Hidden State Extraction API for LoRA

**Description:**
Add API to extract intermediate activations from transformer layers during forward pass, enabling efficient LoRA adapter application at specific modules.

**Use Case:**
K-sparse LoRA routing requires access to hidden states at target modules (q_proj, k_proj, v_proj, o_proj) to apply low-rank adapters without full model recomputation.

**Proposed API:**
```cpp
std::unordered_map<std::string, mlx::core::array>
mlx::nn::Module::forward_with_hooks(...);
```

**Alternatives Considered:**
- Modifying model definition (breaks compatibility)
- Graph inspection (complex, fragile)
- Separate forward passes (slow, memory-intensive)

**Related Projects:**
- AdapterOS: Requires for production MLX backend
- (Other MLX projects using LoRA)

**References:**
- PyTorch forward hooks: https://pytorch.org/docs/stable/generated/torch.nn.Module.html#torch.nn.Module.register_forward_hook
```

---

## 4. Issue Monitoring

### 4.1 Critical Issues

**Filter Criteria:**
- Labels: `bug`, `crash`, `memory-leak`, `C++`, `API`
- Keywords: "segfault", "crash", "undefined behavior", "leak"
- Impact: Affects production deployments

**Current Critical Issues (as of 2025-11-19):**
- [ ] None identified (check weekly)

**How to Monitor:**
```bash
# GitHub CLI search
gh issue list --repo ml-explore/mlx --label bug,C++ --state open

# RSS feed for issues
https://github.com/ml-explore/mlx/issues.atom
```

### 4.2 Performance Issues

**Filter Criteria:**
- Labels: `performance`, `optimization`
- Keywords: "slow", "memory", "latency"

**Relevant Benchmarks:**
- Llama inference speed (tokens/sec)
- Memory usage vs Metal backend
- Cold start time

**How to Monitor:**
```bash
gh issue list --repo ml-explore/mlx --label performance --state open
```

### 4.3 Documentation Issues

**Filter Criteria:**
- Labels: `documentation`, `examples`
- Keywords: "C++", "API", "tutorial"

**Watch for:**
- New C++ examples (show best practices)
- API reference updates
- Migration guides

---

## 5. Pull Request Monitoring

### 5.1 High-Impact PRs

**Priority Labels:**
- `breaking-change`: API changes requiring AdapterOS updates
- `enhancement`: New features
- `bug`: Critical fixes

**Notification Setup:**
```bash
# Watch specific directories
gh api /repos/ml-explore/mlx/subscription \
  -f subscribed=true -f ignored=false

# Custom RSS feed for PRs
https://github.com/ml-explore/mlx/pulls.atom
```

### 5.2 PR Review Checklist

When high-impact PR is merged:
- [ ] Read release notes / changelog
- [ ] Check if AdapterOS code affected
- [ ] Test against new MLX version
- [ ] Update AdapterOS compatibility matrix
- [ ] Update integration plan if needed

**Example High-Impact PRs:**
- C++ API refactor
- LoRA module rewrite
- Memory management changes
- Quantization updates

---

## 6. Release Monitoring

### 6.1 MLX Release Cadence

**Historical Pattern (2025):**
- 0.29.5: Nov 11, 2025
- 0.29.4: Nov 11, 2025
- 0.29.3: Oct 17, 2025
- (Earlier versions throughout 2025)

**Observation:** ~Monthly releases, rapid iteration

### 6.2 Release Review Process

**When new MLX version released:**
1. **Read Changelog**
   - Identify breaking changes
   - Note new features
   - Check bug fixes

2. **Impact Assessment**
   - Does it affect AdapterOS?
   - Required changes: None / Minor / Major
   - Upgrade urgency: Low / Medium / High

3. **Testing**
   - Update local MLX installation
   - Run AdapterOS test suite
   - Benchmark performance
   - Document any issues

4. **Update Documentation**
   - Update compatibility matrix
   - Note breaking changes
   - Update integration plan if needed

### 6.3 Release Notification

**Setup GitHub Notifications:**
```bash
# Watch releases only
gh repo view ml-explore/mlx --watch releases

# Or use RSS
https://github.com/ml-explore/mlx/releases.atom
```

**Email Alerts:**
- GitHub: Settings → Notifications → Watching repositories
- Enable email for releases

---

## 7. Community Monitoring

### 7.1 MLX Discussion Forums

**GitHub Discussions:**
- URL: https://github.com/ml-explore/mlx/discussions
- Watch: C++ category, Q&A

**Reddit:**
- r/MachineLearning (filter: MLX, Apple Silicon)
- r/LocalLLaMA (LoRA fine-tuning discussions)

**Discord/Slack:**
- Check if MLX has official community chat
- Join Apple ML research channels if available

### 7.2 Blog Posts & Tutorials

**Official Apple Sources:**
- Apple Machine Learning Research blog
- WWDC session videos (annual)
- Apple Developer forums

**Community Blogs:**
- Medium (search: "MLX", "Apple Silicon", "LoRA")
- Hugging Face blog
- FastMLX project blog

**How to Track:**
- Google Alerts: "MLX Apple Silicon"
- RSS feeds: Apple ML blog, Hugging Face
- Twitter/X: @AppleMLResearch, #MLX

### 7.3 Academic Papers

**Search Terms:**
- "MLX framework"
- "Apple Silicon machine learning"
- "Unified memory ML"

**Sources:**
- arXiv.org (cs.LG, cs.AI categories)
- Papers with Code (MLX implementations)

**Relevant Papers (as of 2025-11-19):**
- "Profiling Apple Silicon Performance for ML Training" (arXiv:2501.14925)

---

## 8. Competitive Analysis

### 8.1 Alternative Frameworks

Monitor for features AdapterOS could adopt:

| Framework | Platform | Unified Memory | LoRA Support | Watch For |
|-----------|----------|----------------|--------------|-----------|
| **PyTorch** | Cross-platform | ❌ | ✅ (PEFT) | MPS backend improvements |
| **JAX** | Cross-platform | ❌ | ✅ | TPU/GPU optimizations |
| **TensorFlow** | Cross-platform | ❌ | ✅ | Lite Metal delegate |
| **llama.cpp** | Cross-platform | ❌ | ✅ | Apple Silicon optimizations |
| **MLX** | Apple Silicon | ✅ | ✅ | **Our target** |

### 8.2 Feature Gap Analysis

**Quarterly Review:**
- What features do competitors have that MLX lacks?
- What MLX features are unique?
- Should AdapterOS support multiple backends?

**Current Assessment (2025-11-19):**
- **MLX Advantage:** Unified memory (512GB models)
- **Competitor Advantage:** Cross-platform, mature ecosystems
- **AdapterOS Strategy:** MLX for large models, Metal for production

---

## 9. Monitoring Schedule

### 9.1 Daily (Automated)
- [ ] Check GitHub issue notifications
- [ ] Review PR activity (high-impact PRs)
- [ ] Scan release announcements

**Automation:**
```bash
#!/bin/bash
# mlx_monitor.sh - Run via cron daily

# Check for new releases
gh release list --repo ml-explore/mlx --limit 1 | grep -v "Latest"

# Check for critical issues
gh issue list --repo ml-explore/mlx --label bug,crash --state open

# Check for breaking PRs
gh pr list --repo ml-explore/mlx --label breaking-change --state open
```

### 9.2 Weekly (Manual Review)
- [ ] Review issue activity (critical bugs)
- [ ] Check PR merge activity
- [ ] Scan community discussions
- [ ] Update watchlist notes

**Time Commitment:** 30-60 minutes/week

### 9.3 Monthly (Deep Dive)
- [ ] Review release notes in detail
- [ ] Test new MLX version with AdapterOS
- [ ] Update compatibility matrix
- [ ] Review integration plan progress

**Time Commitment:** 2-4 hours/month

### 9.4 Quarterly (Strategic Review)
- [ ] Assess MLX maturity vs plan
- [ ] Reevaluate integration timeline
- [ ] Update feature request priorities
- [ ] Competitive analysis

**Time Commitment:** 4-8 hours/quarter

---

## 10. Action Triggers

### 10.1 Immediate Action Required

**Trigger:** Critical bug affecting AdapterOS
**Action:**
1. Assess impact (blocker? workaround available?)
2. File AdapterOS issue tracking MLX bug
3. Monitor MLX fix progress
4. Test fix when available
5. Update AdapterOS dependencies

**Trigger:** Breaking API change in MLX release
**Action:**
1. Create compatibility branch
2. Update AdapterOS code
3. Run full test suite
4. Update documentation
5. Merge when stable

**Trigger:** New feature enables previously blocked work
**Action:**
1. Update integration plan
2. Reprioritize roadmap
3. Begin implementation
4. Update watchlist status

### 10.2 Planned Action

**Trigger:** MLX 1.0 release (production-ready signal)
**Action:**
1. Prioritize full MLX integration
2. Allocate engineering resources
3. Begin Phase 1 implementation
4. Update project timeline

**Trigger:** mlx-c reaches production status
**Action:**
1. Reevaluate C API vs C++ integration
2. Compare implementation complexity
3. Update integration plan if C API preferred
4. Prototype with mlx-c

---

## 11. Contact Points

### 11.1 MLX Maintainers

**How to reach:**
- GitHub issues (public)
- GitHub discussions (community)
- Email: (check MLX repo MAINTAINERS file)

**When to contact:**
- Critical bugs blocking AdapterOS
- Feature requests with strong use case
- Security vulnerabilities

**Etiquette:**
- Search existing issues first
- Provide minimal reproducible example
- Be respectful of maintainer time

### 11.2 Community Experts

**Find via:**
- GitHub contributors (check commit history)
- Stack Overflow (MLX tag)
- Reddit (active MLX users)
- Discord/Slack (if available)

**How to engage:**
- Ask specific technical questions
- Share AdapterOS progress (build community)
- Contribute back (PRs, documentation)

---

## 12. Watchlist Maintenance

### 12.1 Document Updates

**Update frequency:** After each monitoring session
**Update triggers:**
- New MLX release
- Critical issue identified
- Feature request filed
- Integration milestone reached

**Version Control:**
```bash
# Track watchlist changes
git log docs/MLX_DEVELOPMENT_WATCHLIST.md

# Review recent updates
git diff HEAD~1 docs/MLX_DEVELOPMENT_WATCHLIST.md
```

### 12.2 Review Schedule

**Monthly Review:**
- Remove resolved issues
- Update status indicators
- Add new critical issues
- Adjust priorities

**Quarterly Review:**
- Archive old issues
- Reassess monitoring strategy
- Update contact points
- Prune irrelevant sections

---

## 13. References

### 13.1 MLX Resources
- **Main Repo:** https://github.com/ml-explore/mlx
- **C API Repo:** https://github.com/ml-explore/mlx-c
- **Documentation:** https://ml-explore.github.io/mlx/
- **C API Docs:** https://ml-explore.github.io/mlx-c/

### 13.2 Monitoring Tools
- **GitHub CLI:** https://cli.github.com/
- **RSS Readers:** Feedly, NewsBlur, Inoreader
- **GitHub Notifications:** https://github.com/notifications

### 13.3 Related Documents
- **Integration Plan:** `/Users/star/Dev/aos/docs/MLX_CPP_INTEGRATION_PLAN.md`
- **Stub README:** `/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/README.md`
- **Kernel API:** `/Users/star/Dev/aos/crates/adapteros-lora-kernel-api/src/lib.rs`

---

## 14. Appendix: Quick Commands

```bash
# Install MLX (macOS)
brew install mlx

# Clone MLX repos
git clone https://github.com/ml-explore/mlx.git
git clone https://github.com/ml-explore/mlx-c.git

# Watch MLX releases
gh repo view ml-explore/mlx --watch

# Search MLX issues
gh issue list --repo ml-explore/mlx --search "C++ API"

# Monitor PR activity
gh pr list --repo ml-explore/mlx --state open --label enhancement

# Check latest release
gh release view --repo ml-explore/mlx

# Download release
gh release download --repo ml-explore/mlx

# Test AdapterOS with new MLX
cd /Users/star/Dev/aos
MLX_FORCE_STUB=0 cargo test -p adapteros-lora-mlx-ffi
```

---

**Status:** ✅ Watchlist established, ready for weekly monitoring
**Next Update:** 2025-11-26 (weekly review)
**Maintained by:** Agent 11 - MLX Path Planner
