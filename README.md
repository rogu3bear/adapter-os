# adapterOS

**Deterministic ML inference platform for Apple Silicon**

adapterOS is a Rust-based ML inference platform optimized for Apple Silicon. It provides K-sparse LoRA routing, Metal-optimized kernels, and policy enforcement for production and air-gapped deployments.

---

## What is adapterOS?

adapterOS enables **deterministic multi-adapter inference** on Apple Silicon:

- **K-Sparse LoRA Routing** — Dynamic adapter selection with Q15 quantized gates
- **Deterministic Execution** — Reproducible outputs via HKDF seeding and canonical serialization
- **Policy Enforcement** — Runtime validation with canonical policy packs
- **Zero Network Egress** — Air-gapped serving over Unix domain sockets
- **Multi-Backend** — MLX (primary), CoreML/ANE, Metal kernels

---

## Quick Start

```bash
# Bootstrap CLI launcher (builds/rebuilds aosctl as needed)
./aosctl --rebuild --help

# Initialize
./aosctl db migrate

# Start
./start
```

See [QUICKSTART.md](QUICKSTART.md) for full setup.

---

## QA Visual Gate

The canonical UI quality gate is Playwright-based and runs as dual-browser blocking checks on macOS:

```bash
cd tests/playwright
npm run test:gate:quality -- --project=chromium
npm run test:gate:quality -- --project=webkit
```

Baseline policy:
- Canonical visual baselines are macOS snapshots (`*-darwin.png`).
- Gate execution enforces a snapshot contract precheck that fails on missing active or orphaned baselines.

See [tests/playwright/README.md](tests/playwright/README.md) for lane composition, run artifacts, and troubleshooting.

---

## Requirements

- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust** (stable, see `rust-toolchain.toml`)
- **MLX** — `brew install mlx` (for inference/training)

---

## Documentation

| Document | Description |
|----------|-------------|
| [QUICKSTART.md](QUICKSTART.md) | Get running in minutes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | System design and components |
| [DETERMINISM.md](DETERMINISM.md) | Reproducibility and replay |
| [POLICIES.md](POLICIES.md) | Policy engine and packs |
| [docs/](docs/) | Full documentation index |
| [docs/governance/](docs/governance/) | Governance policy and operations |

---

## License

Proprietary. Copyright © MLNavigator Inc R&D | James KC Auchterlonie. All rights reserved.
