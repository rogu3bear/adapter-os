# ReplayResultDialog Component

**Part of PRD-02: Deterministic Replay Feature**

A comprehensive dialog component for displaying deterministic replay results with comparison views, detailed analysis, and performance statistics.

## Files

- `ReplayResultDialog.tsx` - Main component
- `ReplayResultDialog.example.tsx` - Usage examples
- `../../api/replay-types.ts` - TypeScript type definitions and helpers

## Features

### 1. Three-Tab Interface

#### Comparison Tab
- **Side-by-Side View**: Shows original and replay responses in parallel columns
- **Diff View**: Unified diff highlighting additions/removals
- **Divergence Highlighting**: Visual indicators at character positions where responses diverge
- **Copy Buttons**: Quick clipboard copy for both responses
- **View Mode Toggle**: Switch between side-by-side and diff views

#### Details Tab
- **Match Status Badge**: Color-coded badge (Exact=green, Semantic=yellow, Divergent=orange, Error=red)
- **Divergence Analysis**: Shows position, backend changes, manifest changes
- **Approximation Reasons**: Lists all reasons why replay was approximate
- **Truncation Warnings**: Alerts if response exceeded 64KB storage limit
- **Replay IDs**: Shows both replay_id and original_inference_id

#### Statistics Tab
- **Performance Metrics**: Token counts and latency with comparison to original
- **Latency Comparison**: Visual progress bars comparing original vs replay timing
- **RAG Reproducibility**: Document availability score with progress bar and missing doc list
- **Color-coded Metrics**: Green for better performance, red for worse

### 2. Match Status Display

The component uses color-coded badges for match status:

| Status | Badge Color | Icon | Meaning |
|--------|-------------|------|---------|
| **Exact** | Green | CheckCircle2 | Token-for-token identical |
| **Semantic** | Yellow | AlertCircle | Similar meaning, different words |
| **Divergent** | Orange | TrendingUp | Significantly different |
| **Error** | Red | XCircle | Replay failed with error |

### 3. RAG Reproducibility

If RAG (Retrieval-Augmented Generation) was used in the original inference:

- **Score Bar**: Visual progress bar (0-100%)
- **Document Count**: Shows `matching_docs / total_original_docs`
- **Missing Documents**: List of document IDs that are no longer available
- **Conditional Display**: Only shown if `rag_reproducibility` is present

### 4. Responsive Design

- **Mobile-friendly**: Grid layouts stack on small screens
- **Max Height**: Dialog scrolls at 90vh to prevent overflow
- **Accessible**: ARIA labels on progress bars
- **Toast Notifications**: Success/error feedback for copy operations

## Props

```typescript
interface ReplayResultDialogProps {
  /** Controls dialog visibility */
  open: boolean;

  /** Callback when dialog open state changes */
  onOpenChange: (open: boolean) => void;

  /** Replay response data from API */
  replayResponse: ReplayResponse | null;

  /** Optional callback to view replay history */
  onViewHistory?: () => void;
}
```

## Usage

### Basic Usage

```tsx
import { ReplayResultDialog } from '@/components/chat/ReplayResultDialog';
import { useState } from 'react';

function MyComponent() {
  const [open, setOpen] = useState(false);
  const [result, setResult] = useState(null);

  const handleReplay = async () => {
    const response = await fetch('/api/v1/replay', {
      method: 'POST',
      body: JSON.stringify({ inference_id: 'abc123' }),
    });
    const data = await response.json();
    setResult(data);
    setOpen(true);
  };

  return (
    <>
      <button onClick={handleReplay}>Replay</button>
      <ReplayResultDialog
        open={open}
        onOpenChange={setOpen}
        replayResponse={result}
      />
    </>
  );
}
```

### With History Button

```tsx
<ReplayResultDialog
  open={open}
  onOpenChange={setOpen}
  replayResponse={result}
  onViewHistory={() => navigate(`/replay/history/${result?.original_inference_id}`)}
/>
```

## API Integration

The component expects data matching the backend types:

```typescript
// POST /api/v1/replay
{
  "inference_id": "string",        // Required
  "allow_approximate": boolean,    // Optional, default: false
  "skip_rag": boolean              // Optional, default: false
}

// Response: ReplayResponse
{
  "replay_id": "string",
  "original_inference_id": "string",
  "replay_mode": "exact" | "approximate" | "degraded",
  "response": "string",
  "response_truncated": boolean,
  "match_status": "exact" | "semantic" | "divergent" | "error",
  "original_response": "string",
  "stats": {
    "tokens_generated": number,
    "latency_ms": number,
    "original_latency_ms"?: number
  },
  "divergence"?: {
    "divergence_position"?: number,
    "backend_changed": boolean,
    "manifest_changed": boolean,
    "approximation_reasons": string[]
  },
  "rag_reproducibility"?: {
    "score": number,           // 0.0 - 1.0
    "matching_docs": number,
    "total_original_docs": number,
    "missing_doc_ids": string[]
  }
}
```

## Helper Functions

The `replay-types.ts` file includes helper functions:

```typescript
import {
  getMatchStatusLabel,
  getMatchStatusBadgeVariant,
  getRagReproducibilityPercent,
  formatLatencyDiff,
  canReplay,
} from '@/api/replay-types';

// Get human-readable label
getMatchStatusLabel('exact'); // "Exact Match"

// Get badge variant for UI
getMatchStatusBadgeVariant('exact'); // "success"

// Calculate RAG percentage
getRagReproducibilityPercent(ragData); // 75 (from 0.75 score)

// Format latency with diff
formatLatencyDiff(stats); // "156ms (+6ms, +3.9%)"

// Check if replay is possible
canReplay(availability); // true/false
```

## Styling

The component uses:
- **Tailwind CSS** for styling
- **shadcn/ui** components (Dialog, Tabs, Badge, Progress, Button)
- **lucide-react** icons
- **Dark mode** support via Tailwind dark: classes

### Color Scheme

| Element | Light Mode | Dark Mode |
|---------|------------|-----------|
| Background | `bg-muted/50` | Same |
| Text | `text-foreground` | Same |
| Success | `text-green-600` | `dark:text-green-400` |
| Warning | `text-yellow-600` | `dark:text-yellow-400` |
| Error | `text-red-600` | `dark:text-red-400` |

## Accessibility

- Semantic HTML structure
- ARIA labels on progress bars
- Keyboard navigation support (Tab, Enter, Escape)
- Screen reader friendly
- Focus management within dialog

## Testing

See `ReplayResultDialog.example.tsx` for mock data generators:

```typescript
const mockExactMatch: ReplayResponse = { ... };
const mockDivergent: ReplayResponse = { ... };
const mockWithRAG: ReplayResponse = { ... };
```

## Performance Considerations

- **Tab Lazy Loading**: Tab content only renders when active
- **Memoized Highlights**: Uses `useMemo` for divergence highlighting
- **Efficient Diff**: Only compares at divergence position, not entire text
- **Scroll Containers**: Max heights prevent DOM overflow

## Future Enhancements

Potential improvements (not yet implemented):

- [ ] Token-level diff highlighting (word-by-word)
- [ ] Export replay results as JSON/PDF
- [ ] Inline diff editing to compare custom text
- [ ] Replay chain visualization (multiple replays)
- [ ] Integration with audit trail timeline

## Related Documentation

- [PRD-02: Deterministic Replay](../../docs/prd/PRD-02-deterministic-replay.md)
- [Replay API Reference](../../docs/API_REFERENCE.md#replay-endpoints)
- [Database Schema: replay_executions](../../docs/database-schema.md#replay_executions)
- [Migration 0126: inference_replay_metadata](../../migrations/0126_inference_replay_metadata.sql)
- [Migration 0127: replay_executions](../../migrations/0127_replay_executions.sql)

## Copyright

Copyright JKCA | 2025 James KC Auchterlonie
