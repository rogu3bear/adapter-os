# AOS Runtime Performance

This feature provides a dedicated `adapteros-aos` crate with:

- Memory-mapped `.aos` loading with zero-copy weight access when weights are stored (ZIP Stored)
- Atomic hot-swap to replace adapters in sub-millisecond time
- A simple LRU cache to avoid repeated I/O

## Enabling mmap loading

In server config (`server.server` section):

- `enable_mmap_adapters = true`
- `mmap_cache_size_mb = 512` (example)

## Enabling hot-swap

Set `enable_hot_swap = true` in the same section. When enabled, the API provides:

POST `/api/v1/adapters/:adapter_id/hot-swap` with body:

```
{ "new_path": "/path/to/new_file.aos" }
```

Response includes `swap_time_ms` and `old_adapter` ID (if any).

## Notes

- Zero-copy is possible when `.aos` weights entries use ZIP Stored. For deflated entries, the loader falls back to on-demand decompression without caching the decompressed buffer.
- Hot-swap only replaces the in-memory pointer to the adapter mapping; consumers should dereference through the manager each time to benefit from atomicity.

