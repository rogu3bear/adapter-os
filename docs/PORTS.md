# Port Contract (Local Pane)

AdapterOS local/dev/test networking is standardized to a dedicated pane anchored at `AOS_PORT_PANE_BASE=18080`.

## Canonical Defaults

- `AOS_SERVER_PORT=18080` control plane HTTP/API (backend-served UI)
- `AOS_UI_PORT=18081` UI dev server
- `AOS_PANEL_PORT=18082` service supervisor panel
- `AOS_NODE_PORT=18083` node agent
- `AOS_PROMETHEUS_PORT=18084` metrics/datasource
- `AOS_MODEL_SERVER_PORT=18085` model server (`AOS_MODEL_SERVER_ADDR=http://127.0.0.1:18085`)
- `AOS_CODEGRAPH_PORT=18086` codegraph dev lane
- `AOS_MINIMAL_UI_PORT=18087` minimal UI lane
- `AOS_OTLP_PORT=18088` OTLP endpoint
- `AOS_VAULT_PORT=18089` local vault lane
- `AOS_KMS_EMULATOR_PORT=18090` KMS emulator lane
- `AOS_POSTGRES_PORT=18091` local postgres lane
- `AOS_LOCALSTACK_PORT=18092` localstack lane
- `PW_SERVER_PORT` dynamic test lane starts at `18180`

## Source of Truth

- Shell/runtime contract: `scripts/lib/ports.sh`
- Rust canonical defaults: `crates/adapteros-core/src/defaults.rs`
- API-type mirror defaults: `crates/adapteros-api-types/src/defaults.rs`

## Drift Guard

Run:

```bash
bash scripts/contracts/check_port_contract.sh
```

This fails if legacy localhost defaults reappear outside allowlisted files.
