# Training Metrics Comparison Visualization

**Component:** `MetricsComparison.tsx`
**Agent:** Agent 17
**Created:** 2025-11-19
**Purpose:** Interactive visualization for comparing training job metrics

---

## Overview

The `MetricsComparison` component provides comprehensive visual comparison of multiple training jobs, enabling users to:

- Compare loss curves across different configurations
- Analyze training performance and resource usage
- Identify best-performing models and optimal hyperparameters
- Detect overfitting via train/validation gap analysis
- Export charts for documentation and reporting

---

## Features

### 1. Loss Curve Overlay
- **Training Loss:** Solid lines showing training loss progression
- **Validation Loss:** Dashed lines showing validation loss (toggle-able)
- **Best Epoch Indicator:** Vertical reference line marking optimal checkpoint
- **Smoothing:** Moving average filter to reduce noise
- **Log Scale:** Logarithmic y-axis for better visualization of small differences

### 2. Performance Comparison
- **Tokens/Second:** Area chart showing throughput over time
- **Throughput Trends:** Identify performance degradation or improvements
- **Comparative Analysis:** Overlay multiple jobs for side-by-side comparison

### 3. Resource Usage
- **GPU Utilization:** Line chart showing GPU usage percentage (0-100%)
- **Memory Usage:** GPU memory consumption in GB
- **Resource Efficiency:** Compare memory/compute trade-offs between configurations

### 4. Statistical Analysis
- **Best Loss:** Minimum loss achieved and corresponding epoch
- **Convergence Rate:** Loss reduction per epoch (slope analysis)
- **Average Throughput:** Mean tokens/second across training
- **Best Job Indicator:** Automatic highlighting of top performer

### 5. Interactive Features
- **Job Visibility Toggle:** Click legend badges to show/hide specific jobs
- **Scale Switching:** Toggle between linear and logarithmic scales
- **Validation Toggle:** Show/hide validation loss overlay
- **Smoothing Control:** Enable/disable curve smoothing
- **Chart Export:** Download charts as PNG/SVG (planned)

---

## Usage

### Basic Usage

```typescript
import { MetricsComparison } from '@/components/training';
import { TrainingJob, TrainingMetrics } from '@/api/types';

function TrainingDashboard() {
  const [jobs, setJobs] = useState<TrainingJob[]>([]);
  const [metricsHistory, setMetricsHistory] = useState<Map<string, TrainingMetrics[]>>(new Map());

  useEffect(() => {
    // Fetch jobs from API
    const loadJobs = async () => {
      const jobList = await apiClient.listTrainingJobs();
      setJobs(jobList);

      // Fetch metrics history for each job
      const history = new Map();
      for (const job of jobList) {
        const metrics = await apiClient.getTrainingMetrics(job.id);
        history.set(job.id, metrics);
      }
      setMetricsHistory(history);
    };

    loadJobs();
  }, []);

  return (
    <MetricsComparison
      jobs={jobs}
      metricsHistory={metricsHistory}
    />
  );
}
```

### With Real-Time Updates

```typescript
import { usePolling } from '@/hooks/realtime/usePolling';

function LiveMetricsComparison() {
  const { data: jobs } = usePolling(
    () => apiClient.listTrainingJobs(),
    'normal'
  );

  const metricsHistory = useMemo(() => {
    const history = new Map();
    jobs?.forEach(job => {
      // Fetch or generate metrics timeline
      history.set(job.id, job.metricsTimeline || []);
    });
    return history;
  }, [jobs]);

  return (
    <MetricsComparison
      jobs={jobs || []}
      metricsHistory={metricsHistory}
    />
  );
}
```

---

## Props

### `MetricsComparisonProps`

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `jobs` | `TrainingJob[]` | Yes | Array of training jobs to compare |
| `metricsHistory` | `Map<string, TrainingMetrics[]>` | No | Map of job ID to metrics timeline (epoch-by-epoch) |
| `className` | `string` | No | Additional CSS classes |

### `TrainingJob` Interface

```typescript
interface TrainingJob {
  id: string;
  adapter_name: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  progress?: number;
  current_epoch?: number;
  total_epochs?: number;
  current_loss?: number;
  tokens_per_second?: number;
  config?: TrainingConfig;
  metrics?: TrainingMetrics;
}
```

### `TrainingMetrics` Interface

```typescript
interface TrainingMetrics {
  loss: number;
  tokens_per_second: number;
  learning_rate: number;
  current_epoch: number;
  total_epochs: number;
  progress_pct: number;
  validation_loss?: number;
  gpu_utilization?: number;
  memory_usage?: number;
}
```

---

## Chart Types

### 1. Loss Curve (Line Chart)
- **X-axis:** Epoch number
- **Y-axis:** Loss value (linear or log scale)
- **Lines:** One per job (solid = training, dashed = validation)
- **Features:** Smoothing, best epoch indicator, interactive legend

### 2. Performance Chart (Area Chart)
- **X-axis:** Epoch number
- **Y-axis:** Tokens per second
- **Areas:** Filled areas with transparency for each job
- **Features:** Trend analysis, comparative throughput

### 3. GPU Utilization (Line Chart)
- **X-axis:** Epoch number
- **Y-axis:** GPU usage percentage (0-100%)
- **Lines:** One per job
- **Features:** Resource efficiency analysis

### 4. Memory Usage (Line Chart)
- **X-axis:** Epoch number
- **Y-axis:** Memory in GB
- **Lines:** One per job
- **Features:** Memory footprint comparison

### 5. Convergence Analysis (Bar Chart)
- **X-axis:** Job name
- **Y-axis:** Loss reduction per epoch
- **Bars:** Convergence rate for each job
- **Features:** Quick comparison of training efficiency

---

## Color Palette

Uses the **Tol Bright** color-blind friendly palette:

```typescript
const JOB_COLORS = [
  '#4477AA', // Blue
  '#EE6677', // Red
  '#228833', // Green
  '#CCBB44', // Yellow
  '#66CCEE', // Cyan
  '#AA3377', // Purple
  '#BBBBBB', // Grey
];
```

### Why Tol Bright?
- **Accessibility:** Distinguishable for all types of color blindness
- **Print-Safe:** Works well in grayscale
- **Professional:** Suitable for technical documentation

---

## Performance Characteristics

### Data Points
- **Supported:** Up to 5,000 data points per job
- **Recommended:** 100-500 epochs per job
- **Max Jobs:** 2-7 jobs (beyond 7, colors cycle)

### Optimization
- **Memoization:** All derived data uses `useMemo` hooks
- **Lazy Rendering:** Charts only render when visible
- **Debouncing:** Toggle controls debounced to prevent thrashing
- **Virtual Scrolling:** Not needed for typical datasets

### Browser Compatibility
- Chrome/Edge: 90+
- Firefox: 88+
- Safari: 14+
- Mobile: iOS Safari 14+, Chrome Mobile

---

## Interactive Features

### 1. Job Visibility Toggle
```typescript
// Click badge to toggle
<Badge onClick={() => toggleJob(job.id)}>
  {visibleJobs.has(job.id) ? <Eye /> : <EyeOff />}
  {job.adapter_name}
</Badge>
```

### 2. Scale Toggle
```typescript
<Switch
  id="log-scale"
  checked={logScale}
  onCheckedChange={setLogScale}
/>
```

### 3. Smoothing Control
- **Algorithm:** Moving average with configurable window
- **Window Size:** 5 epochs (default)
- **Effect:** Reduces noise, reveals trends

### 4. Chart Export (Planned)
```typescript
// Future implementation with html2canvas
const exportChart = (chartId: string) => {
  const chartElement = document.getElementById(chartId);
  html2canvas(chartElement).then(canvas => {
    const link = document.createElement('a');
    link.download = `${chartId}-${Date.now()}.png`;
    link.href = canvas.toDataURL();
    link.click();
  });
};
```

---

## Statistical Analysis

### 1. Best Epoch Detection
```typescript
const bestEpoch = losses.indexOf(Math.min(...losses));
```

### 2. Convergence Rate
```typescript
// Loss reduction per epoch
const convergenceRate = (firstLoss - lastLoss) / losses.length;
```

### 3. Overfitting Detection
```typescript
// Train/validation gap
const overfitting = validationLoss > trainingLoss * 1.2; // 20% threshold
```

### 4. Performance Metrics
```typescript
const avgThroughput = throughputs.reduce((a, b) => a + b, 0) / throughputs.length;
```

---

## Example Scenarios

### Scenario 1: Hyperparameter Tuning
**Goal:** Compare rank/alpha configurations

```typescript
const jobs = [
  { id: '1', adapter_name: 'r8a16', config: { rank: 8, alpha: 16 } },
  { id: '2', adapter_name: 'r16a32', config: { rank: 16, alpha: 32 } },
  { id: '3', adapter_name: 'r32a64', config: { rank: 32, alpha: 64 } },
];
```

**Analysis:**
- Compare final loss values
- Evaluate convergence speed
- Assess memory/performance trade-offs

### Scenario 2: Learning Rate Comparison
**Goal:** Find optimal learning rate

```typescript
const jobs = [
  { id: '1', config: { learning_rate: 0.0001 } },
  { id: '2', config: { learning_rate: 0.0003 } },
  { id: '3', config: { learning_rate: 0.001 } },
];
```

**Analysis:**
- Identify fastest convergence
- Detect instability (oscillations)
- Find sweet spot

### Scenario 3: Overfitting Analysis
**Goal:** Detect generalization issues

```typescript
<MetricsComparison
  jobs={jobs}
  metricsHistory={history}
  // Enable validation overlay
/>
// Toggle "Validation" switch ON
```

**Analysis:**
- Compare train/validation gap
- Identify early stopping point
- Prevent overfitting

---

## Accessibility

### ARIA Labels
```typescript
<button aria-label={`Toggle visibility for ${job.adapter_name}`}>
  {/* Toggle button */}
</button>
```

### Keyboard Navigation
- **Tab:** Navigate between controls
- **Enter/Space:** Toggle switches and buttons
- **Arrow Keys:** Navigate chart tooltips (future)

### Color Contrast
- All text meets WCAG AA standards
- Color palette tested for color blindness
- Tooltips have high-contrast backgrounds

---

## Integration with TrainingMonitor

```typescript
import { MetricsComparison } from '@/components/training';
import { TrainingMonitor } from '@/components/TrainingMonitor';

function TrainingDashboard() {
  const [selectedJob, setSelectedJob] = useState<string | null>(null);
  const [jobs, setJobs] = useState<TrainingJob[]>([]);

  return (
    <div className="space-y-6">
      {/* Overview comparison */}
      <MetricsComparison jobs={jobs} />

      {/* Detailed monitor for selected job */}
      {selectedJob && (
        <TrainingMonitor jobId={selectedJob} />
      )}
    </div>
  );
}
```

---

## Backend Requirements

### API Endpoints

#### List Training Jobs
```
GET /v1/training/jobs
Response: TrainingJob[]
```

#### Get Job Metrics History
```
GET /v1/training/jobs/:id/metrics
Response: TrainingMetrics[]
```

### Metrics Storage
- Store metrics per epoch in `training_metrics` table
- Index on `job_id` and `epoch` for fast retrieval
- Prune old metrics after job completion (configurable retention)

---

## Future Enhancements

### Phase 1 (Q1 2026)
- [ ] Chart export (PNG/SVG)
- [ ] Custom metric selection (user-defined KPIs)
- [ ] Annotation support (mark important epochs)

### Phase 2 (Q2 2026)
- [ ] Real-time streaming (WebSocket updates)
- [ ] Statistical tests (ANOVA, t-tests)
- [ ] Confidence intervals for metrics

### Phase 3 (Q3 2026)
- [ ] A/B test framework integration
- [ ] Automated hyperparameter suggestions
- [ ] Report generation (PDF export)

---

## Troubleshooting

### Issue: Charts not rendering
**Cause:** Missing Recharts dependency
**Fix:**
```bash
pnpm add recharts@^2.15.2
```

### Issue: Metrics not loading
**Cause:** API endpoint returns wrong format
**Fix:** Validate response matches `TrainingMetrics[]` interface

### Issue: Performance degradation
**Cause:** Too many data points
**Fix:** Implement downsampling for large datasets

```typescript
const downsample = (data: number[], targetPoints: number) => {
  const factor = Math.ceil(data.length / targetPoints);
  return data.filter((_, i) => i % factor === 0);
};
```

---

## References

- [Recharts Documentation](https://recharts.org/)
- [Tol Color Schemes](https://personal.sron.nl/~pault/)
- [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)
- [AdapterOS Training Pipeline](../../../docs/TRAINING_PIPELINE.md)

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
