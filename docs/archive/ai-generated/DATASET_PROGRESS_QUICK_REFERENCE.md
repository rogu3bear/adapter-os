# Dataset Progress SSE - Quick Reference

## What Was Added

A real-time Server-Sent Events (SSE) endpoint that broadcasts dataset upload and validation progress to connected clients.

## Endpoint

```
GET /v1/datasets/upload/progress?dataset_id={optional_id}
```

## Event Structure

```json
{
  "dataset_id": "uuid",
  "event_type": "upload|validation|statistics",
  "current_file": "filename.ext",
  "percentage_complete": 45.5,
  "total_files": 10,
  "files_processed": 4,
  "message": "Status description",
  "timestamp": "2025-11-19T09:30:15.123Z"
}
```

## Files Changed

1. **state.rs**
   - Added `DatasetProgressEvent` struct
   - Added `dataset_progress_tx: Option<Arc<broadcast::Sender<DatasetProgressEvent>>>` to AppState
   - Added `with_dataset_progress()` builder method

2. **handlers/datasets.rs**
   - Added `send_progress_event()` helper
   - Enhanced `upload_dataset()` with progress events
   - Enhanced `validate_dataset()` with progress events
   - Implemented `dataset_upload_progress()` SSE handler

3. **routes.rs**
   - Added route: `/v1/datasets/upload/progress`
   - Added to OpenAPI documentation

## Quick Client Example

### JavaScript
```javascript
const eventSource = new EventSource('/v1/datasets/upload/progress');
eventSource.onmessage = (e) => {
  const progress = JSON.parse(e.data);
  console.log(`${progress.percentage_complete}% - ${progress.message}`);
};
```

### React Hook
```typescript
import { useEffect, useState } from 'react';

export function useDatasetProgress(datasetId?: string) {
  const [progress, setProgress] = useState(null);

  useEffect(() => {
    const url = `/v1/datasets/upload/progress${datasetId ? `?dataset_id=${datasetId}` : ''}`;
    const es = new EventSource(url);
    es.onmessage = (e) => setProgress(JSON.parse(e.data));
    return () => es.close();
  }, [datasetId]);

  return progress;
}
```

### cURL
```bash
curl -N http://localhost:8080/v1/datasets/upload/progress?dataset_id=abc123
```

## Enabling in Application

In server startup (main.rs):

```rust
use tokio::sync::broadcast;

// Create channel
let (dataset_progress_tx, _) = broadcast::channel(1024);

// Attach to AppState
let app_state = AppState::new(/* ... */)
    .with_dataset_progress(Arc::new(dataset_progress_tx));
```

## Event Timeline Example

### Upload Sequence
1. 0% - "Starting dataset upload..."
2. 10% - "Uploaded file1.json (1.2 MB)"
3. 20% - "Uploaded file2.json (2.1 MB)"
4. ... continues for each file

### Validation Sequence
1. 0% - "Starting dataset validation..." (shows total_files)
2. 50% - "Validated file1.json"
3. 100% - "Validated file2.json"

## Notes

- Uses tokio broadcast channels (multi-subscriber pattern)
- Non-blocking: doesn't slow down upload/validation
- Supports filtering by dataset_id via query parameter
- Handles disconnections gracefully
- Memory efficient: ~200 bytes per event

## Testing

```bash
# Terminal 1: Listen
curl -N http://localhost:8080/v1/datasets/upload/progress

# Terminal 2: Upload
curl -X POST -F "name=test" -F "file=@data.jsonl" \
  http://localhost:8080/v1/datasets/upload

# Terminal 3: Validate (if needed)
curl -X POST http://localhost:8080/v1/datasets/{id}/validate
```

## Compilation Status

✅ Code compiles successfully with `cargo check -p adapteros-server-api`

## Related Patterns in Codebase

Similar SSE implementation already exists:
- `handlers/git.rs:file_changes_stream()` - File change events
- `handlers/telemetry.rs` - Telemetry streaming

This follows the same pattern for consistency.
