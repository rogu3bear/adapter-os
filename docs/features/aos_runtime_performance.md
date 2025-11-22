# AOS Runtime Performance

This feature provides a dedicated `adapteros-aos` crate with:

- Memory-mapped `.aos` loading with zero-copy weight access (AOS 3.0 format with 64-byte header)
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

- Zero-copy is guaranteed for AOS 3.0 artifacts because weights start at byte 64 (after the cache-line aligned header). Legacy v1/v2 formats are also supported with automatic format detection.
- Hot-swap only replaces the in-memory pointer to the adapter mapping; consumers should dereference through the manager each time to benefit from atomicity.

## Format Support

| Format | Header Size | Zero-Copy | Notes |
|--------|-------------|-----------|-------|
| AOS 3.0 | 64 bytes | Yes | Current format, cache-line aligned |
| AOS 2.0 | 268 bytes | Yes | Legacy format with larger header |
| AOS 1.0 | 8 bytes | Yes | Simple legacy format |

See [AOS Format Specification](../AOS_FORMAT.md) for details.
