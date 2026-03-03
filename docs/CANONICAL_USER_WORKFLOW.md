# Canonical User Workflow

This document defines the primary operator workflow in canonical product grammar.

## Flow

1. Upload a **document** in `/documents`.
2. Convert the document into a **dataset** and confirm the active **dataset version**.
3. Start a **training job** in `/training` using the selected dataset version.
4. Wait for the training job to produce an **adapter version**.
5. Open `/adapters` to inspect lineage and promotion state.
6. Start **chat** in `/chat` with the selected adapter version.

## Notes

- If a document is already represented in an existing dataset, reuse that dataset and continue from its latest dataset version.
- If a training job already exists for the dataset version, reuse that training job instead of creating a parallel run.
