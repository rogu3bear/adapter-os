# Detection Signal

Signals observed:

- `vm_stat` and `top` indicated high system load and active compression/swap activity.
- Synthetic pressure trigger behavior:
  - `zsh:ulimit:68: setrlimit failed: invalid argument`
  - `unexpected_pass 230686720`
  - `synthetic_memory_pressure_exit=0`
- Eviction probe result:
  - `curl -X POST /v1/lifecycle/evict ...` returned `Not Found`

Interpretation:

- Diagnostic telemetry confirmed pressure context; synthetic trigger path is environment-limited on this host shell profile.
