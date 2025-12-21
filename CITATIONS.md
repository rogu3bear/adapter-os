# Citations Format

**Purpose:** Standard citation format for referencing code locations in AdapterOS documentation

---

## Citation Pattern

Use pattern: `[source: crates/adapteros-server/src/main.rs L173-L218]`

Format: `[source: <file-path> L<start>-L<end>]`

---

## Examples

### Single Line
```
[source: crates/adapteros-core/src/hash.rs L42]
```

### Line Range
```
[source: crates/adapteros-lora-router/src/lib.rs L150-L175]
```

### Multiple Citations
```
The router implementation [source: crates/adapteros-lora-router/src/lib.rs L50-L80]
uses Q15 quantization [source: crates/adapteros-lora-router/src/q15.rs L10-L45].
```

---

## Guidelines

1. Use absolute paths from repository root
2. Include line numbers for precise references
3. Update citations when refactoring code
4. Prefer ranges for multi-line references

---

See [AGENTS.md](AGENTS.md) for additional documentation standards.
