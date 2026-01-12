# Trace a Run (Upload -> Train -> Infer)

This guide shows how to trace a single run end-to-end using `correlation_id`.
All commands assume a local server at `http://localhost:3000`.

If auth is enabled, export a token and keep the Authorization header.
If you run `AOS_DEV_NO_AUTH=1 ./start up`, you can omit it.

## Setup

```bash
export AOS_BASE_URL="http://localhost:3000"
export AOS_TOKEN="<token>"
export CORR_ID="$(uuidgen | tr 'A-Z' 'a-z')"
```

## 1) Upload a dataset and capture `dataset_id`

```bash
curl -s -X POST "$AOS_BASE_URL/v1/datasets" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "X-Request-ID: $CORR_ID" \
  -F "name=trace-dataset" \
  -F "format=jsonl" \
  -F "files=@/path/to/data.jsonl" \
  | tee /tmp/dataset.json

DATASET_ID=$(jq -r '.dataset_id' /tmp/dataset.json)
```

You can also start from an existing dataset ID if you already have one.

## 2) Find the training job for that dataset

```bash
curl -s "$AOS_BASE_URL/v1/training/jobs?dataset_id=$DATASET_ID" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | tee /tmp/jobs.json

JOB_ID=$(jq -r '.jobs[0].id' /tmp/jobs.json)
```

## 3) Fetch job details to get `correlation_id`, `adapter_id`, and artifact path

```bash
curl -s "$AOS_BASE_URL/v1/training/jobs/$JOB_ID" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | tee /tmp/job.json

CORRELATION_ID=$(jq -r '.correlation_id' /tmp/job.json)
ADAPTER_ID=$(jq -r '.adapter_id' /tmp/job.json)
AOS_PATH=$(jq -r '.aos_path // .artifact_path' /tmp/job.json)
```

`AOS_PATH` points to the packaged adapter artifact on disk.

## 4) Run inference and verify correlation in logs

Use a minimal prompt to avoid logging sensitive user text.

```bash
curl -s -X POST "$AOS_BASE_URL/v1/infer" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"prompt":"trace","max_tokens":8,"adapters":["'$ADAPTER_ID'"]}' \
  | tee /tmp/infer.json
```

Now search logs for the correlation_id:

```bash
rg "$CORRELATION_ID" var/logs
```

Expected log signatures include:
- Dataset upload: "Dataset upload recorded"
- Training start: "Started training job"
- Worker train: "Starting LoRA training"
- Inference: "Inference correlation resolved"
- Packaging: "Adapter packaged successfully"

## 5) Fetch metrics and report endpoints

```bash
curl -s "$AOS_BASE_URL/v1/training/jobs/$JOB_ID/metrics" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | tee /tmp/metrics.json

curl -s "$AOS_BASE_URL/v1/training/jobs/$JOB_ID/report" \
  -H "Authorization: Bearer $AOS_TOKEN" \
  | tee /tmp/report.json
```

## Common failure signatures

- `UPLOAD_SESSION_FAILED`: chunked upload session failed; retry from session creation.
- `Training job not found`: `GET /v1/training/jobs/{job_id}` or metrics/report called with the wrong ID.
- `Training report not found`: training finished without a report artifact.
- `Worker not initialized`: inference failed before a worker was ready.
- `No eligible worker found`: adapters not loaded or backend capacity mismatch.
- `Failed to resolve correlation_id for adapter`: adapter missing a training job link or tenant mismatch.
