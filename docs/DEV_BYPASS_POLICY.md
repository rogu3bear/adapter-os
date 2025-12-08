# Dev Bypass Policy

Environment matrix for `VITE_ENABLE_DEV_BYPASS` and `/login?dev=true`:

| Environment | Default | Notes |
| --- | --- | --- |
| Dev | ON (implicit via `import.meta.env.DEV`) | For local/demo flows only; banner shown. |
| Staging | OFF by default | May be turned ON temporarily for demos; ensure banner is visible. |
| Production | OFF | Do not enable unless explicitly approved and time-bounded. |

Backend TTL recommendation: if the backend supports a dedicated dev-bypass TTL, set it shorter than the normal token TTL (for example 1–2h) to reduce risk for demo/admin sessions.

Dev-bypass sessions are limited to demo/admin use and always render the banner in the shell.

MLNavigator Inc 2025-12-08.

