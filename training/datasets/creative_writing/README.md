# Creative writing starter dataset for adapterOS

Location: `training/datasets/creative_writing/`

This dataset is a starter set of creative-writing style pairs in JSONL format using
the input/target schema expected by dataset ingest.

## File

- `creative_writing_style_v1.positive.jsonl`
  - One JSON object per line
  - Fields:
    - `input`: prompt / instruction
    - `target`: desired creative response
    - optional `metadata` can be added later if needed

## Suggested ingest command

```bash
./aosctl dataset ingest \
  training/datasets/creative_writing/creative_writing_style_v1.positive.jsonl \
  --format jsonl \
  --name creative-writing-style-v1 \
  --description "Creative writing style dataset starter"
```

A `manifest.json` is included for tooling that expects repository metadata.

Start a control-plane job using the dataset version from ingest/validation:

```bash
./aosctl dataset versions <dataset-id>
./aosctl train start <repo-id> \
  --base-model-id <base-model-id> \
  --dataset-version-ids <dataset-version-id> \
  --adapter-name creative-writing-style \
  --adapter-type creative_writing \
  --backend-policy auto \
  --backend auto
```

Replace `<dataset-version-id>` with the id returned by `aosctl dataset ingest` or
`aosctl dataset versions`.
