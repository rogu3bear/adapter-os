# Wishlist

Items here are aspirational — not planned, not promised, not prioritized.

## Runtime Agent Framework

The codebase has 16 Claude Code agent definitions (`.claude/agents/`) that serve as development tooling — they help build AdapterOS but don't run inside it. The product itself has no autonomous agent runtime.

What exists today:
- **NodeAgent** (`adapteros-node/src/agent.rs`): process supervisor for worker spawning, not an agent framework
- **Orchestration UI** (`adapteros-ui/src/pages/agents.rs`): viewer for multi-adapter routing sessions, not agent orchestration

What's missing: a runtime framework where agents can receive goals, plan steps, execute tools, observe results, and iterate — all within the determinism substrate, auditable, air-gap safe.

The `.claude/agents/` definitions (integration, dedup, infrastructure) sketch the *kinds* of agents the platform would eventually host. The patterns map naturally to operational concerns:
- **Integration agents** → runtime agents that detect and repair contract drift between services
- **Dedup/quality agents** → continuous codebase health monitoring
- **Infrastructure agents** (contract-gate, spec-sync, artifact-shepherd) → CI/CD automation that runs inside the platform rather than as external tooling

Key constraints any runtime agent framework must respect:
- Deterministic execution (seed isolation, replay compatibility)
- Audit trail (every agent action is a logged, signed event)
- Air-gap safe (no network egress during execution)
- Tenant isolation (agents operate within tenant boundaries)
- Policy enforcement (agents are subject to the same policy packs as inference)
