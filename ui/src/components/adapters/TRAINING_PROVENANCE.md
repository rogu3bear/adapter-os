# Training Provenance Panel

## Overview

The `TrainingSnapshotPanel` component displays training provenance information for adapters, showing which documents and configurations were used during training. This provides transparency and auditability for adapter training.

## Features

- **Collection Information**: Shows the collection name if the adapter was trained from a collection
- **Training Metadata**: Displays training date and job ID
- **Document List**: Lists all documents used in training with:
  - Document name and hash
  - Page count
  - Inline PDF viewer access
- **Chunk Manifest Hash**: Shows the hash of the chunk manifest for verification

## Integration

The panel is integrated into the Adapter Detail Page as a new "Provenance" tab:

```tsx
// /Users/mln-dev/Dev/adapter-os/ui/src/pages/Adapters/AdapterDetailPage.tsx
import { TrainingSnapshotPanel } from '@/components/adapters';

<TabsContent value="provenance" className="mt-6">
  <TrainingSnapshotPanel adapterId={adapterId} />
</TabsContent>
```

## API Endpoint

The component fetches data from:

```
GET /v1/adapters/{adapter_id}/training-snapshot
```

**Response Format:**

```typescript
interface TrainingSnapshot {
  id: string;
  adapter_id: string;
  training_job_id: string;
  collection_id: string | null;
  collection_name: string | null;
  documents: Array<{
    doc_id: string;
    doc_name: string;
    doc_hash: string;
    page_count: number;
  }>;
  chunk_manifest_hash: string;
  created_at: string;
}
```

**Status Codes:**
- `200 OK`: Training snapshot found
- `404 Not Found`: No training snapshot available (not an error - adapter may not be trained)
- `500 Internal Server Error`: Server error

## Usage Example

```tsx
import { TrainingSnapshotPanel } from '@/components/adapters';

function MyAdapterView() {
  return (
    <div className="space-y-4">
      <h2>Adapter Details</h2>

      {/* Other adapter information */}

      <TrainingSnapshotPanel adapterId="my-adapter-id" />
    </div>
  );
}
```

## States

### Loading State
Shows animated skeleton placeholder while fetching data.

### Empty State
Displays a friendly message when no training provenance is available:
- Icon indicator
- Explanation that adapter may not be trained through the pipeline

### Data State
Shows full provenance information with:
- Collection badge (if applicable)
- Training date
- Job ID
- Scrollable document list with click-to-view functionality

## PDF Viewer Integration

Each document in the list has a view button that opens the `PDFViewer` component in a modal dialog. Users can:
- Navigate pages
- Zoom in/out
- Download the document
- Close and return to the provenance view

## Database Schema

The component relies on the `training_provenance_snapshots` table documented in `/Users/mln-dev/Dev/adapter-os/docs/TRAINING_PROVENANCE.md`.

## Error Handling

- Network errors display toast notifications
- 404 responses are handled gracefully (empty state)
- Component is resilient to missing or partial data
- PDF loading errors show user-friendly messages

## Future Enhancements

Potential improvements:
- Export provenance report as JSON/PDF
- Compare provenance across adapter versions
- Show training hyperparameters and loss curves
- Link to training job logs
- Highlight differences between training snapshots
