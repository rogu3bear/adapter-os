# adapteros-db

SQLite with migrations, adapter registry, and atomic dual-write.

## Patterns

- **Dual-write**: Critical operations write to primary + audit log atomically
- **Migrations**: Add to `migrations/`, run `./aosctl db migrate`, update `signatures.json`
- **Test isolation**: Use `TestDb::new()` which creates temp database; it auto-cleans

## Testing

```bash
cargo test -p adapteros-db --test atomic_dual_write_tests
```
