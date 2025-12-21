# Dev setup quick note (agent run)
- DB setup: `cargo run -p adapteros-orchestrator -- db migrate`; then `cargo run -p adapteros-orchestrator -- init-tenant --id default --uid 1000 --gid 1000`.
- Backend: set `AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx`, `AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3`, optional `RUST_LOG=info`; start with `cargo run -p adapteros-server-api`.
- UI: `cd ui && pnpm install && pnpm dev` (Vite dev server on 5173; `/dashboard`, `/training`, `/adapters`, `/chat`).
- Metal verification: `cargo build -p adapteros-lora-kernel-mtl --features metal-backend`; hash with `b3sum crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib` and compare to manifest `kernel_hash`.
