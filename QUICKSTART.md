# adapterOS Quick Start

Get adapterOS running on macOS in a few steps.

---

## Prerequisites

- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **MLX** — `brew install mlx`
- **Production macOS builds** (`cargo build --release --workspace --exclude adapteros-fuzz --features production-macos`) require:
  - **Full Xcode** — Command Line Tools alone are not enough for `xcrun metal` or `xcodebuild -downloadComponent MetalToolchain`
  - **Active Xcode developer dir** — `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`
  - **CMake** — `brew install cmake`

---

## 1. Bootstrap CLI Launcher

```bash
./aosctl --rebuild --help
```

---

## 2. Initialize Database

```bash
./aosctl db migrate
```

---

## 3. Model (Required For Default `./start`)

Download an MLX model and set the path:

```bash
# Example: Qwen3.5 27B
huggingface-cli download Qwen/Qwen3.5-27B \
  --include "*.safetensors" "*.json" \
  --local-dir var/models/Qwen3.5-27B

export AOS_MODEL_PATH=var/models/Qwen3.5-27B
```

Or use `./scripts/download-model.sh` to provision and update `.env`.

Startup preflight now fails fast if the model directory is missing/incomplete or incompatible with the worker manifest (`config.json`, `tokenizer.json`, `tokenizer_config.json`, and hash compatibility checks).

If you only need backend/UI startup (no inference worker), you can skip model setup and run:

```bash
./start --skip-worker
```

---

## 4. Start

```bash
./start
```

Server runs on port 18080. UI is served from the same port.

---

## Development Mode

Bypass auth for UI iteration:

```bash
AOS_DEV_NO_AUTH=1 ./start
```

Run a standalone UI dev server (proxying backend routes) when working on frontend-only changes:

```bash
bash scripts/ui-dev.sh
# or directly:
cd crates/adapteros-ui && trunk serve
```

---

## Verify

```bash
xcrun --find metal
cmake --version
./start preflight
./aosctl doctor
./aosctl preflight
```

---

## Next Steps

- [ARCHITECTURE.md](ARCHITECTURE.md) — System design
- [docs/CONFIGURATION.md](docs/CONFIGURATION.md) — Configuration reference
- [docs/OPERATIONS.md](docs/OPERATIONS.md) — Operations guide
