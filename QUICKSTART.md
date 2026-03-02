# adapterOS Quick Start

Get adapterOS running on macOS in a few steps.

---

## Prerequisites

- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **MLX** — `brew install mlx`

---

## 1. Build

```bash
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl
```

---

## 2. Initialize Database

```bash
./aosctl db migrate
```

---

## 3. Model (Optional)

Download an MLX model and set the path:

```bash
# Example: Llama 3.2 3B
huggingface-cli download mlx-community/Llama-3.2-3B-Instruct-4bit \
  --include "*.safetensors" "*.json" \
  --local-dir var/models/Llama-3.2-3B-Instruct-4bit

export AOS_MLX_FFI_MODEL=var/models/Llama-3.2-3B-Instruct-4bit
```

Or use `./aosctl models seed` if configured.

---

## 4. Start

```bash
./start
```

Server runs on port 8080. UI is served from the same port.

---

## Development Mode

Bypass auth for UI iteration:

```bash
AOS_DEV_NO_AUTH=1 ./start
```

---

## Verify

```bash
./aosctl doctor
./aosctl preflight
```

---

## Next Steps

- [ARCHITECTURE.md](ARCHITECTURE.md) — System design
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) — Configuration reference
- [docs/OPERATIONS.md](docs/OPERATIONS.md) — Operations guide
