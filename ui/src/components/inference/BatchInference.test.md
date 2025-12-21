# Batch Inference Testing Guide

## Overview
Batch inference allows users to process multiple prompts simultaneously with shared configuration.

## Features Implemented

### 1. API Integration
- **Location**: `/Users/star/Dev/aos/ui/src/api/client.ts`
- **Method**: `batchInfer(data: BatchInferRequest, cancelToken?: AbortSignal): Promise<BatchInferResponse>`
- **Endpoint**: `/api/batch/infer`
- **Request Format**:
  ```typescript
  {
    requests: [
      {
        id: "batch-timestamp-0",
        prompt: "Write a Python function...",
        max_tokens: 100,
        temperature: 0.7,
        top_k: 50,
        top_p: 0.9,
        seed: undefined,
        require_evidence: false,
        adapters: ["adapter-id"]
      },
      // ... more requests
    ]
  }
  ```
- **Response Format**:
  ```typescript
  {
    responses: [
      {
        id: "batch-timestamp-0",
        response: {
          text: "def fibonacci(n): ...",
          token_count: 50,
          latency_ms: 120,
          finish_reason: "stop",
          trace: { ... }
        },
        error: undefined
      },
      // ... more responses
    ]
  }
  ```

### 2. BatchResults Component
- **Location**: `/Users/star/Dev/aos/ui/src/components/inference/BatchResults.tsx`
- **Features**:
  - Expandable table rows showing full prompts and responses
  - Status badges (Success/Error/Pending)
  - Copy to clipboard for responses
  - Retry functionality for failed items
  - Export buttons (JSON/CSV)
  - Summary statistics (X completed, Y errors, Z pending)

### 3. Batch Input UI
- **Location**: `/Users/star/Dev/aos/ui/src/components/InferencePlayground.tsx` (batch mode)
- **Features**:
  - Textarea for manual prompt entry (one per line)
  - CSV file upload support
  - Text file upload support
  - Validation of all prompts before submission
  - Batch size warning (>100 prompts)
  - Shared configuration preview

### 4. Export Functionality
- **JSON Export**: Complete batch results with metadata
  ```json
  {
    "batchSize": 5,
    "timestamp": "2025-01-19T...",
    "config": { ... },
    "results": [
      {
        "id": "batch-...",
        "prompt": "...",
        "response": "...",
        "token_count": 50,
        "latency_ms": 120,
        "finish_reason": "stop",
        "error": null
      }
    ]
  }
  ```

- **CSV Export**: Spreadsheet-compatible format
  ```csv
  ID,Prompt,Status,Response,Token Count,Latency (ms),Finish Reason,Error
  batch-1,"Write...",Success,"def fibonacci...",50,120,stop,""
  batch-2,"Explain...",Error,"",0,0,"","API timeout"
  ```

### 5. Progress Tracking
- Real-time status updates
- Success/error counters
- Individual item status badges
- Toast notifications for completion

### 6. Error Handling
- Partial failure support (some items succeed, some fail)
- Validation errors displayed before submission
- Individual retry capability for failed items
- Detailed error messages in expandable rows

## Testing Scenarios

### Test 1: Basic Batch Inference
1. Navigate to Inference Playground
2. Select "Batch" mode
3. Enter 3-5 prompts (one per line)
4. Click "Run Batch Inference"
5. Verify all prompts are processed
6. Check success/error badges

### Test 2: CSV Upload
1. Create a CSV file:
   ```csv
   Prompt,Expected
   "Write a hello world function",code
   "Explain machine learning",explanation
   "What is 2+2?",math
   ```
2. Upload the CSV file
3. Verify prompts are loaded
4. Run batch inference
5. Check results

### Test 3: Validation
1. Enter prompts with various issues:
   - Empty prompt
   - Very long prompt (>50K chars)
   - Prompts with special characters
2. Verify validation errors are shown
3. Fix errors and resubmit

### Test 4: Export
1. Run a batch inference with 5+ prompts
2. Click "Export JSON"
3. Verify JSON file downloads with correct format
4. Click "Export CSV"
5. Verify CSV file can be opened in Excel/Numbers

### Test 5: Retry Failed Items
1. Run batch inference (some should fail if server has issues)
2. Click retry button on a failed item
3. Verify individual item is retried
4. Check updated result

### Test 6: Large Batch (Performance)
1. Create a text file with 50 prompts
2. Upload the file
3. Run batch inference
4. Monitor progress and completion time
5. Verify all results are displayed

### Test 7: Configuration Sharing
1. Set custom configuration (temperature, max_tokens, etc.)
2. Enter multiple prompts
3. Run batch inference
4. Verify all prompts use the same configuration
5. Check configuration in exported JSON

## Known Limitations

1. **Batch Size**: Recommended maximum of 100 prompts per batch
2. **API Endpoint**: Backend must support `/api/batch/infer` endpoint
3. **Progress Updates**: No real-time progress during batch processing (all-or-nothing)
4. **CSV Parsing**: Simple CSV parser - assumes prompts in first column
5. **Memory**: Large batches may consume significant memory in browser

## API Requirements

The backend must implement the `/api/batch/infer` endpoint with:
- POST method
- Accept `BatchInferRequest` format
- Return `BatchInferResponse` format
- Handle partial failures gracefully
- Support cancellation via AbortSignal

## Future Enhancements

1. **Streaming Progress**: Real-time updates as each item completes
2. **Advanced CSV Parsing**: Support for multi-column CSV with configuration per row
3. **Batch Templates**: Save and reuse common batch configurations
4. **Parallel vs Sequential**: Option to control batch processing strategy
5. **Filter/Sort Results**: Advanced filtering in results table
6. **Comparison Mode**: Compare results across different configurations
