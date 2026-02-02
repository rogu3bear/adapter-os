# adapteros-ui

Leptos 0.7 CSR app targeting `wasm32-unknown-unknown`.

## Gotchas

- **No `println!`** - Use `web_sys::console::log_1` or `leptos::logging::log!`
- **No `std::time`** - Use `gloo_timers` or `web_sys::Performance`
- **Async in components** - Use `create_local_resource`, not `spawn_local` for data fetching
- **State updates** - Signals are reactive; direct mutation won't trigger re-renders

## Testing

```bash
cargo test -p adapteros-ui --lib  # Native tests only; WASM tests require wasm-pack
```

## CSS

Pure CSS in `dist/`. No Tailwind. Follow Liquid Glass tiers in `dist/glass.css` header.
