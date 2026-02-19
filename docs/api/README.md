# API

API route map and OpenAPI spec.

---

## Contents

| File | Purpose |
|------|---------|
| [ROUTE_MAP.md](ROUTE_MAP.md) | Route inventory by tier (auto-generated) |
| [openapi.json](openapi.json) | OpenAPI 3.0 specification |

---

## Regenerate

```bash
./scripts/dev/generate_route_map.sh
cargo run -p adapteros-server -- --generate-openapi
```
