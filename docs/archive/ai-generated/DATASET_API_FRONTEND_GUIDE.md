# Dataset API - Frontend Integration Guide

**Quick reference for frontend developers integrating dataset upload and management.**

---

## Authentication

All endpoints require JWT authentication:

```typescript
const headers = {
  'Authorization': `Bearer ${token}`
};
```

---

## Upload Dataset (Simple)

For files under 10MB, use the standard upload endpoint:

```typescript
async function uploadDataset(
  name: string,
  files: File[],
  description?: string,
  format: string = 'jsonl'
): Promise<UploadDatasetResponse> {
  const formData = new FormData();
  formData.append('name', name);
  if (description) formData.append('description', description);
  formData.append('format', format);

  files.forEach(file => {
    formData.append('files', file);
  });

  const response = await fetch('/v1/datasets/upload', {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${token}` },
    body: formData
  });

  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error);
  }

  return response.json();
}
```

**Response:**
```typescript
interface UploadDatasetResponse {
  schema_version: string;
  dataset_id: string;
  name: string;
  description?: string;
  file_count: number;
  total_size_bytes: number;
  format: string;
  hash: string;
  created_at: string;
}
```

---

## Monitor Upload Progress (SSE)

Track real-time upload progress via Server-Sent Events:

```typescript
function watchUploadProgress(
  datasetId: string,
  onProgress: (progress: DatasetProgressEvent) => void,
  onError?: (error: Error) => void
): () => void {
  const url = `/v1/datasets/upload/progress?dataset_id=${datasetId}`;

  // Note: EventSource doesn't support custom headers natively
  // Use polyfill or fetch API with ReadableStream for auth
  const eventSource = new EventSource(url);

  eventSource.onmessage = (event) => {
    try {
      const progress: DatasetProgressEvent = JSON.parse(event.data);
      onProgress(progress);
    } catch (error) {
      onError?.(error as Error);
    }
  };

  eventSource.onerror = (error) => {
    onError?.(new Error('SSE connection error'));
    eventSource.close();
  };

  // Return cleanup function
  return () => eventSource.close();
}

interface DatasetProgressEvent {
  dataset_id: string;
  event_type: 'upload' | 'validation' | 'statistics';
  current_file?: string;
  percentage_complete: number;
  total_files?: number;
  files_processed?: number;
  message: string;
  timestamp: string;
}
```

**React Example:**
```tsx
const [progress, setProgress] = useState<number>(0);
const [message, setMessage] = useState<string>('');

useEffect(() => {
  if (!datasetId) return;

  const unsubscribe = watchUploadProgress(
    datasetId,
    (event) => {
      setProgress(event.percentage_complete);
      setMessage(event.message);
    },
    (error) => console.error('Progress error:', error)
  );

  return unsubscribe;
}, [datasetId]);
```

---

## List Datasets

Get all datasets with optional filters:

```typescript
interface ListDatasetsQuery {
  limit?: number;        // default: 50, max: 100
  offset?: number;       // default: 0
  format?: string;       // filter by format
  validation_status?: 'pending' | 'valid' | 'invalid';
}

async function listDatasets(
  query: ListDatasetsQuery = {}
): Promise<DatasetResponse[]> {
  const params = new URLSearchParams();
  if (query.limit) params.append('limit', query.limit.toString());
  if (query.offset) params.append('offset', query.offset.toString());
  if (query.format) params.append('format', query.format);
  if (query.validation_status) params.append('validation_status', query.validation_status);

  const response = await fetch(`/v1/datasets?${params}`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });

  return response.json();
}

interface DatasetResponse {
  schema_version: string;
  dataset_id: string;
  name: string;
  description?: string;
  file_count: number;
  total_size_bytes: number;
  format: string;
  hash: string;
  storage_path: string;
  validation_status: 'pending' | 'valid' | 'invalid';
  validation_errors?: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}
```

---

## Get Dataset Details

```typescript
async function getDataset(datasetId: string): Promise<DatasetResponse> {
  const response = await fetch(`/v1/datasets/${datasetId}`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });

  if (!response.ok) {
    throw new Error('Dataset not found');
  }

  return response.json();
}
```

---

## Get Dataset Files

```typescript
async function getDatasetFiles(
  datasetId: string
): Promise<DatasetFileResponse[]> {
  const response = await fetch(`/v1/datasets/${datasetId}/files`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });

  return response.json();
}

interface DatasetFileResponse {
  schema_version: string;
  file_id: string;
  file_name: string;
  file_path: string;
  size_bytes: number;
  hash: string;
  mime_type?: string;
  created_at: string;
}
```

---

## Validate Dataset

Validate dataset integrity and format:

```typescript
async function validateDataset(
  datasetId: string,
  checkFormat: boolean = true
): Promise<ValidateDatasetResponse> {
  const response = await fetch(`/v1/datasets/${datasetId}/validate`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ check_format: checkFormat })
  });

  return response.json();
}

interface ValidateDatasetResponse {
  schema_version: string;
  dataset_id: string;
  is_valid: boolean;
  validation_status: string;
  errors?: string[];
  validated_at: string;
}
```

**With Progress Monitoring:**
```typescript
async function validateDatasetWithProgress(
  datasetId: string,
  onProgress: (progress: DatasetProgressEvent) => void
): Promise<ValidateDatasetResponse> {
  // Start validation
  const validationPromise = validateDataset(datasetId);

  // Monitor progress
  const unsubscribe = watchUploadProgress(
    datasetId,
    (event) => {
      if (event.event_type === 'validation') {
        onProgress(event);
      }
    }
  );

  try {
    return await validationPromise;
  } finally {
    unsubscribe();
  }
}
```

---

## Get Dataset Statistics

```typescript
async function getDatasetStatistics(
  datasetId: string
): Promise<DatasetStatisticsResponse> {
  const response = await fetch(`/v1/datasets/${datasetId}/statistics`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });

  if (response.status === 404) {
    throw new Error('Statistics not yet computed');
  }

  return response.json();
}

interface DatasetStatisticsResponse {
  schema_version: string;
  dataset_id: string;
  num_examples: number;
  avg_input_length: number;
  avg_target_length: number;
  language_distribution?: Record<string, number>;
  file_type_distribution?: Record<string, number>;
  total_tokens: number;
  computed_at: string;
}
```

---

## Preview Dataset

Get preview of dataset contents:

```typescript
async function previewDataset(
  datasetId: string,
  limit: number = 10
): Promise<DatasetPreview> {
  const response = await fetch(
    `/v1/datasets/${datasetId}/preview?limit=${limit}`,
    {
      headers: { 'Authorization': `Bearer ${token}` }
    }
  );

  return response.json();
}

interface DatasetPreview {
  dataset_id: string;
  format: string;
  total_examples: number;
  examples: any[]; // Format-dependent
}
```

---

## Delete Dataset

**Admin only:**

```typescript
async function deleteDataset(datasetId: string): Promise<void> {
  const response = await fetch(`/v1/datasets/${datasetId}`, {
    method: 'DELETE',
    headers: { 'Authorization': `Bearer ${token}` }
  });

  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error);
  }
}
```

---

## Complete Upload Flow Example

```typescript
async function uploadAndValidateDataset(
  name: string,
  files: File[],
  onProgress: (progress: number, message: string) => void
): Promise<DatasetResponse> {
  // Step 1: Upload
  onProgress(0, 'Starting upload...');
  const uploadResult = await uploadDataset(name, files);

  // Step 2: Monitor upload progress
  const progressUnsubscribe = watchUploadProgress(
    uploadResult.dataset_id,
    (event) => {
      onProgress(event.percentage_complete, event.message);
    }
  );

  // Wait for upload to complete
  await new Promise(resolve => setTimeout(resolve, 1000));
  progressUnsubscribe();

  // Step 3: Validate
  onProgress(100, 'Upload complete. Validating...');
  const validationResult = await validateDataset(uploadResult.dataset_id);

  if (!validationResult.is_valid) {
    throw new Error(`Validation failed: ${validationResult.errors?.join(', ')}`);
  }

  // Step 4: Get final dataset details
  onProgress(100, 'Validation complete');
  return getDataset(uploadResult.dataset_id);
}
```

---

## Error Handling

All endpoints return errors in this format:

```typescript
interface ErrorResponse {
  error: string;
  code: string;
  details?: any;
}
```

**Common error codes:**
- `BAD_REQUEST` - Invalid request (400)
- `UNAUTHORIZED` - Missing/invalid token (401)
- `FORBIDDEN` - Insufficient permissions (403)
- `NOT_FOUND` - Dataset not found (404)
- `PAYLOAD_TOO_LARGE` - File too large (413)
- `INTERNAL_ERROR` - Server error (500)

**Example:**
```typescript
try {
  await uploadDataset(name, files);
} catch (error) {
  if (error.response?.status === 413) {
    alert('File too large. Maximum 100MB per file, 500MB total.');
  } else if (error.response?.status === 403) {
    alert('You do not have permission to upload datasets.');
  } else {
    alert(`Upload failed: ${error.message}`);
  }
}
```

---

## Permissions

| Operation | Admin | Operator | SRE | Compliance | Viewer |
|-----------|-------|----------|-----|------------|--------|
| List/View | ✓ | ✓ | ✓ | ✓ | ✓ |
| Upload | ✓ | ✓ | ✗ | ✗ | ✗ |
| Validate | ✓ | ✓ | ✗ | ✓ | ✗ |
| Delete | ✓ | ✗ | ✗ | ✗ | ✗ |

---

## Size Limits

- **Per File:** 100MB
- **Total Upload:** 500MB
- **Chunked Upload:** Use for files > 10MB (see DATASET_API_IMPLEMENTATION.md)

---

## Supported Formats

- `jsonl` - JSON Lines (one JSON object per line)
- `json` - JSON array of objects
- `txt` - Plain text
- `patches` - Code patches
- `custom` - Custom format

---

## Best Practices

1. **Always validate after upload:**
   ```typescript
   const result = await uploadDataset(...);
   await validateDataset(result.dataset_id);
   ```

2. **Monitor progress for large uploads:**
   ```typescript
   watchUploadProgress(datasetId, updateProgressBar);
   ```

3. **Check statistics before training:**
   ```typescript
   const stats = await getDatasetStatistics(datasetId);
   if (stats.num_examples < 100) {
     alert('Dataset too small for training');
   }
   ```

4. **Handle errors gracefully:**
   ```typescript
   try {
     await operation();
   } catch (error) {
     showErrorNotification(error.message);
   }
   ```

5. **Clean up SSE connections:**
   ```typescript
   useEffect(() => {
     const unsubscribe = watchUploadProgress(...);
     return unsubscribe; // Cleanup on unmount
   }, [datasetId]);
   ```

---

## Testing

Mock responses for testing:

```typescript
// jest.mock
jest.mock('./api', () => ({
  uploadDataset: jest.fn().mockResolvedValue({
    dataset_id: 'test-123',
    name: 'Test Dataset',
    file_count: 3,
    total_size_bytes: 1024000,
    format: 'jsonl',
    hash: 'abc123',
    created_at: '2025-11-19T10:00:00Z'
  }),

  validateDataset: jest.fn().mockResolvedValue({
    dataset_id: 'test-123',
    is_valid: true,
    validation_status: 'valid',
    validated_at: '2025-11-19T10:05:00Z'
  })
}));
```

---

## See Also

- **Full API Documentation:** `/Users/star/Dev/aos/DATASET_API_IMPLEMENTATION.md`
- **RBAC Documentation:** `/Users/star/Dev/aos/docs/RBAC.md`
- **Training Pipeline:** `/Users/star/Dev/aos/CLAUDE.md` (Training section)
