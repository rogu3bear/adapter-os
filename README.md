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
# Build
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl

# Initialize
./aosctl db migrate

# Start
./start
```

See [QUICKSTART.md](QUICKSTART.md) for full setup.

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

---

## License

Proprietary. Copyright © MLNavigator Inc R&D | James KC Auchterlonie. All rights reserved.
