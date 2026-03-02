# CLI_GUIDE

aosctl. Source: `crates/adapteros-cli`.

---

## Help

```bash
./aosctl --help
./aosctl <subcommand> --help
```

---

## Command Structure

```mermaid
flowchart TB
    subgraph Top["aosctl"]
        DB["db migrate"]
        DOC["doctor"]
        PRE["preflight"]
        MOD["models seed, list"]
        AD["adapter list"]
        CHAT["chat"]
        SERVE["serve"]
        TRAIN["train start, status, list"]
        EXPLAIN["explain <code>"]
    end

    subgraph Backend["Backend Interaction"]
        API["HTTP :8080"]
        UDS["UDS worker.sock"]
    end

    DB --> API
    DOC --> API
    PRE --> API
    MOD --> API
    AD --> API
    CHAT --> API
    SERVE --> UDS
    TRAIN --> API
    EXPLAIN --> API
```

---

## Common Commands

| Command | Purpose | Source |
|---------|---------|--------|
| `aosctl db migrate` | Run migrations | commands/db.rs |
| `aosctl doctor` | Health check | commands/doctor.rs |
| `aosctl preflight` | Preflight checks | commands/preflight.rs |
| `aosctl models seed` | Seed models from dir | commands/models.rs |
| `aosctl models list` | List models | commands/models.rs |
| `aosctl adapter list` | List adapters | commands/adapters.rs |
| `aosctl chat` | Interactive chat | commands/chat.rs |
| `aosctl serve` | Start worker (UDS) | commands/serve.rs |
| `aosctl train start` | Start training job | commands/train_cli.rs |
| `aosctl train-docs` | Train on docs | commands/train_docs.rs |
| `aosctl explain <code>` | Explain error code | commands/explain.rs |

---

## Build

```bash
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl
```

---

## Manual

`crates/adapteros-cli/docs/aosctl_manual.md`
