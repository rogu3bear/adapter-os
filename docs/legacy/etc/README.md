# Etc (Legacy Root)

Status: legacy docs-only directory.

## Canonical Configuration Paths

- Control plane runtime config: `configs/cp.toml`
- Supervisor deployment configs: `deploy/supervisor.yaml`, `deploy/supervisor.local.yaml`
- Service-level install examples: `scripts/aos-supervisor.service`, `scripts/backup/cron.example`

## Archived Files

- `docs/legacy/etc/cp.toml`
- `docs/legacy/etc/supervisor.yaml`

## Guidance

- Do not recreate a root `etc/` directory for repository runtime configuration.
- Keep this directory as historical documentation only until full retirement.
