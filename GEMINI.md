# GEMINI.md: adapterOS Project Context

This document provides essential context for an AI assistant working on the adapterOS project.

## 1. Project Overview

**adapterOS** is a sophisticated, Rust-based ML inference platform specifically designed and optimized for **Apple Silicon (macOS)**. Its core mission is to provide a high-performance, secure, and **deterministic** environment for running machine learning models.

The key architectural tenets are:
*   **Determinism**: The same model input will always produce the exact same output, which is critical for auditable and reproducible AI. This is achieved through controlled execution, pre-compiled kernels, and cryptographic seeding.
*   **Local-First & Secure**: The platform is designed for on-premise or air-gapped deployments. It avoids network egress during inference, communicating via Unix domain sockets.
*   **Performance on Apple Silicon**: It leverages Apple's native technologies like **Metal**, **CoreML**, and **MLX** for hardware-accelerated performance.
*   **Modular "Adapters"**: The system uses a concept of LoRA (Low-Rank Adaptation) adapters. Instead of running multiple large models, it uses a single base model and swaps small, specialized "adapters" to tailor it for specific tasks (e.g., code review, summarization).
*   **Monorepo Structure**: The project is a large Rust workspace composed of over 60 interdependent crates, located in the `/crates` directory. This modular design separates concerns like the CLI (`adapteros-cli`), the API server (`adapteros-server-api`), the database layer (`adapteros-db`), and various ML components.

## 2. Building and Running

The project includes a server backend, a web UI, and a command-line tool (`aosctl`).

### Prerequisites

*   Apple Silicon Mac (macOS 14+)
*   Xcode Command Line Tools
*   Rust (version specified in `rust-toolchain.toml`)
*   Node.js v20+ and pnpm v8+

### Step-by-Step Execution Guide

1.  **Clone the Repository:**
    ```bash
    git clone https://github.com/rogu3bear/adapter-os.git
    cd adapter-os
    ```

2.  **Build the Project:**
    This command compiles all crates in the workspace.
    ```bash
    cargo build --release --workspace
    ```

3.  **Download a Default ML Model:**
    The system requires a base model to function. A helper script is provided.
    ```bash
    ./scripts/download_model.sh
    ```

4.  **Initialize the Database:**
    This sets up the environment file, creates a symlink for the command-line tool, and runs the necessary database migrations.
    ```bash
    cp .env.example .env
    ln -sf target/release/aosctl ./aosctl
    ./aosctl db migrate
    ```

5.  **Start the System:**
    The primary script `./start` manages the backend server and the UI.
    ```bash
    ./start
    ```

6.  **Access the Application:**
    The web UI will be available at `http://localhost:8080`.

## 3. Development Conventions

Adhere to these conventions to maintain code quality and consistency.

*   **Primary Tooling**:
    *   **Run All Services**: `start`
    *   **Build**: `cargo build --release --workspace`
    *   **Testing**: `cargo test --workspace`
    *   **Formatting**: `cargo fmt --all`
    *   **Linting**: `cargo clippy --workspace -- -D warnings`

*   **Database Migrations**:
    *   The project uses `refinery` for SQL migrations, located in the `migrations/` directory.
    *   Apply migrations using the CLI: `./aosctl db migrate`.
    *   After creating or editing migrations, sign them: `./scripts/sign_migrations.sh`.

*   **Code Style & Architecture**:
    *   The project uses pre-commit hooks to enforce architectural rules. Install with `cp .githooks/pre-commit-architectural .git/hooks/pre-commit`.
    *   The codebase is highly modular. Infer the purpose of a crate from its name (e.g., `adapteros-crypto`, `adapteros-policy`).
    *   Dependencies are managed centrally in the root `Cargo.toml` under `[workspace.dependencies]`.

*   **Command-Line Interface**:
    *   The `aosctl` binary is the primary tool for administrative tasks (database, tenants, etc.). It is built from the `adapteros-cli` crate.

*   **Configuration**:
    *   System configuration is managed via `.toml` files in the `/configs` directory and environment variables defined in `.env`.
