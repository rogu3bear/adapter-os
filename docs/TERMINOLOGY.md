# TERMINOLOGY

Key terms used in adapterOS. Code is authoritative.

---

## Node

**Node** = cluster node (one machine in a distributed setup). Not Node.js.

- **Node agent** (`aos-node`, `adapteros-node`) — Per-host daemon that spawns workers, reports status, and handles federation on that machine.
- **Node ID** — Unique identifier for a cluster node (e.g. from Ed25519 public key hash).
- **Node.js** — Not used in this sense. The UI uses Leptos/WASM; tooling may use Node.js for Playwright, pnpm, etc., but "node" in adapterOS docs always means cluster node unless explicitly stated.

---

## Other terms

See [ARCHITECTURE.md](ARCHITECTURE.md) for topology, [POLICIES.md](POLICIES.md) for policy terms.
