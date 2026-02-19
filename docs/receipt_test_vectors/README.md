# Receipt Test Vectors

Canonical test vectors for receipt digest verification. Used by integration tests.

---

## v7

Schema: `RECEIPT_SCHEMA_V7` (`adapteros-core`).

| Vector | Input | Expected digest |
|--------|-------|-----------------|
| minimal | minimal.input.json | minimal.expected_receipt_digest_hex.txt |
| typical | typical.input.json | typical.expected_receipt_digest_hex.txt |
| citations_equipment | citations_equipment.input.json | citations_equipment.expected_receipt_digest_hex.txt |

---

## Consumers

- `adapteros-crypto/tests/receipt_payload_vectors.rs`
- `adapteros-core/tests/canonical_receipt_serialization_vectors.rs`
- `adapteros-cli/tests/receipt_payload_vectors.rs`
- `adapteros-server-api/tests/receipt_payload_vectors.rs`
