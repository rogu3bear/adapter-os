# Training Comparison Component

**Purpose:** Side-by-side comparison of multiple training jobs with performance metrics, configuration differences, and export capabilities.

**Location:** `/Users/star/Dev/aos/ui/src/components/training/TrainingComparison.tsx`

---

## Features

### 1. Job Selection Interface
- Multi-select job picker (2-4 jobs)
- Filter by status: all, completed, running, failed, cancelled
- Sort by: date, loss, duration
- Shows basic job info: name, status, date, final loss
- Visual checkbox selection
- Maximum 4 jobs limit with toast notification

### 2. Summary Statistics
- Grid layout (1-4 columns, responsive)
- Per-job card showing:
  - Adapter name
  - Final loss (4 decimal precision)
  - Tokens/second
  - Status badge (color-coded)

### 3. Performance Metrics Comparison
Side-by-side table with following metrics:
- **Final Loss** - Lower is better (green up arrow)
- **Status** - completed/running/failed/cancelled
- **Progress** - Percentage completion
- **Epochs** - Total epochs configured
- **Current Epoch** - Last completed epoch
- **Learning Rate** - Exponential notation
- **Tokens/Second** - Integer format, higher is better
- **Duration** - Human-readable (hours, minutes, seconds)

**Comparison Logic:**
- Baseline: First selected job
- Green up arrow (TrendingUp): Better than baseline
- Red down arrow (TrendingDown): Worse than baseline
- Gray minus (Minus): Equal to baseline
- Yellow highlight: Different values across jobs
- Identical badge: All jobs have same value

### 4. Configuration Comparison
Displays all hyperparameters side-by-side:
- Rank, Alpha, Targets
- Batch Size, Learning Rate, Epochs
- Max Seq Length, Warmup Steps
- Gradient Accumulation Steps
- Category, Scope, Framework ID/Version
- Repo ID, Commit SHA

**Features:**
- Grid layout with per-job columns
- Yellow highlight for different values
- Warning badge for parameter differences
- Identical badge when all match
- Target layers section with badge display

### 5. Hyperparameter Diff Table
Detailed comparison table:
- Parameter name
- Values for each job
- Difference indicator
- Color-coded rows (yellow for differences)
- Alert icon for mismatches

### 6. Export Capabilities

**CSV Export:**
- Headers: Metric, Job1, Job2, Job3, Job4
- Rows: All performance metrics
- Filename: `training-comparison-{timestamp}.csv`
- Format: Standard comma-separated

**JSON Export:**
- Structured comparison object
- Includes timestamp, job metadata, metrics, config
- Filename: `training-comparison-{timestamp}.json`
- Format: Pretty-printed (2-space indent)

**Markdown Report:**
- Full report with sections:
  - Title and generation timestamp
  - Summary (numbered list of jobs)
  - Metrics comparison table
  - Configuration comparison (with warning emojis)
- Filename: `training-comparison-{timestamp}.md`
- Format: GitHub-flavored markdown

---

## Usage

```tsx
import { TrainingComparison } from '@/components/training';

function TrainingDashboard() {
  const [jobs, setJobs] = useState<TrainingJob[]>([]);

  return (
    <TrainingComparison
      jobs={jobs}
      onClose={() => console.log('Closed')}
    />
  );
}
```

---

## Props

```typescript
interface TrainingComparisonProps {
  jobs: TrainingJob[];    // All available training jobs
  onClose?: () => void;   // Optional close callback
}
```

---

## Comparison Metrics

| Metric | Format | Comparison Logic |
|--------|--------|------------------|
| Final Loss | 4 decimals | Lower is better |
| Status | String | N/A |
| Progress | Percentage | N/A |
| Epochs | Integer | N/A |
| Current Epoch | Integer | N/A |
| Learning Rate | Exponential (2 decimals) | N/A |
| Tokens/Second | Integer | Higher is better |
| Duration | Human-readable (h/m/s) | Shorter is better |

---

## Configuration Fields

12 hyperparameter fields compared:
1. Rank
2. Alpha
3. Batch Size
4. Learning Rate
5. Epochs
6. Max Seq Length
7. Warmup Steps
8. Gradient Accumulation Steps
9. Category
10. Scope
11. Framework ID
12. Framework Version

Plus: Targets (array of layer names)

---

## State Management

```typescript
const [selectedJobIds, setSelectedJobIds] = useState<string[]>([]);
const [isSelectOpen, setIsSelectOpen] = useState(false);
const [filterStatus, setFilterStatus] = useState<string>('all');
const [sortBy, setSortBy] = useState<'date' | 'loss' | 'duration'>('date');
```

---

## Maximum Jobs Supported

**Hard Limit:** 4 jobs
- Enforced with toast notification
- Selection UI shows "({selected}/4)"
- Responsive grid adapts: 1 column (mobile), 2 (tablet), 4 (desktop)

---

## Export Formats

### CSV Structure
```csv
Metric,Job1,Job2,Job3,Job4
Final Loss,0.1234,0.1456,0.1123,0.1345
Status,completed,completed,failed,running
...
```

### JSON Structure
```json
{
  "timestamp": "2025-11-19T10:30:00Z",
  "jobs": [
    {
      "id": "job-123",
      "name": "rust-adapter-v1",
      "status": "completed",
      "created_at": "2025-11-19T08:00:00Z",
      "metrics": {
        "Final Loss": 0.1234,
        "Tokens/Second": 1500
      },
      "config": {
        "rank": 16,
        "alpha": 32
      }
    }
  ]
}
```

### Markdown Structure
```markdown
# Training Job Comparison Report

Generated: 11/19/2025, 10:30:00 AM

## Summary

1. **rust-adapter-v1** - completed (100%)
2. **python-adapter-v2** - failed (75%)

## Metrics Comparison

| Metric | rust-adapter-v1 | python-adapter-v2 |
|--------|-----------------|-------------------|
| Final Loss | 0.1234 | 0.1456 |
...

## Configuration Comparison

**Rank ⚠️:** 16, 12
**Alpha:** 32, 32
...
```

---

## Color Coding

- **Success (Green):** Better performance, completed status
- **Error (Red):** Worse performance, failed status
- **Warning (Yellow):** Different values, attention needed
- **Info (Blue):** Running status
- **Neutral (Gray):** Identical values, cancelled status

---

## Responsive Behavior

- **Mobile (< 768px):** Single column layout, stacked cards
- **Tablet (768px - 1024px):** 2 columns for summary cards
- **Desktop (> 1024px):** 4 columns for full comparison

---

## Citations

- **CLAUDE.md L87-93:** Error handling with `Result<T, AosError>`
- **CLAUDE.md L95-97:** Logging with `tracing` (used `toast` for UI feedback)
- **types.ts L1016-1092:** TrainingJob and TrainingConfig types
- **UI Components:** Card, Badge, Table, Dialog, Checkbox, Select from shadcn/ui

---

## Future Enhancements

1. **Visual Charts:** Add line charts for loss curves
2. **Statistical Analysis:** Mean, median, std dev across jobs
3. **Recommendation Engine:** Suggest best configuration
4. **Diff Highlighting:** Git-style diff for config changes
5. **Permalink Sharing:** Generate shareable comparison URLs
6. **Auto-refresh:** Real-time updates for running jobs
