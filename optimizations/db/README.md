# DB Optimization Coordination

All database optimizations MUST be registered in [`optimizations/db/registry.toml`](optimizations/db/registry.toml:1).

This registry is the coordination mechanism that enables:

- **Parallel work** without conflicting changes (explicit `touches`, `depends_on`, `conflicts_with`).
- **Safe rollout** by requiring `canary`, `rollback`, and `impact_assessment` fields.
- **Change management integration** via PR checklist enforcement.

See:

- [`docs/db/DB_OPTIMIZATION_COORDINATION.md`](docs/db/DB_OPTIMIZATION_COORDINATION.md:1)
- [`docs/runbooks/DB_OPTIMIZATION_ROLLOUT.md`](docs/runbooks/DB_OPTIMIZATION_ROLLOUT.md:1)

