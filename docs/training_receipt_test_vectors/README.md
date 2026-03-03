# Training Receipt Test Vectors

Canonical test vectors for training receipt digest verification.

---

## v1

Schema: `TRAINING_RECEIPT_DIGEST_SCHEMA_V1` (`adapteros-core`).

| Vector | Input | Expected digest |
|--------|-------|-----------------|
| minimal | minimal.input.json | minimal.expected_training_receipt_digest_hex.txt |
| typical | typical.input.json | typical.expected_training_receipt_digest_hex.txt |
| reordered_phases | reordered_phases.input.json | reordered_phases.expected_training_receipt_digest_hex.txt |
| multi_phase | multi_phase.input.json | multi_phase.expected_training_receipt_digest_hex.txt |

---

## Consumers

- `adapteros-core/tests/canonical_training_receipt_serialization_vectors.rs`
