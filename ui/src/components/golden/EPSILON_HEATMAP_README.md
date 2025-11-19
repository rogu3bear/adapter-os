# Epsilon Heatmap Visualization

**Agent 10 Deliverable: Diff Visualization - Metrics & Epsilon Heatmap**

## Overview

This module provides comprehensive epsilon heatmap visualization for layer-by-layer golden run comparisons in AdapterOS. The components enable deep analysis of numerical divergences between golden baseline runs and current test runs.

## Components Created

### 1. EpsilonHeatmap.tsx

**Purpose:** Interactive heatmap visualization of layer divergences

**Features:**
- Color-coded grid visualization (green → yellow → red based on relative error)
- Grouped by adapter prefix for easy navigation
- Zoom and pan controls for large layer counts
- Click-to-view detailed layer information
- Adapter filtering integration
- Real-time statistics summary

**Performance:**
- Optimized for 100+ layers
- Responsive grid layout
- Efficient color calculation

**Usage:**
```tsx
<EpsilonHeatmap
  divergences={layerDivergences}
  tolerance={1e-5}
  adapterFilter="adapter:lora"
  onAdapterClick={(prefix) => handleFilterChange(prefix)}
/>
```

### 2. LayerComparisonTable.tsx

**Purpose:** Sortable, filterable table view of layer divergences

**Features:**
- Multi-column sorting (layer ID, relative error, L2 errors)
- Search filtering by layer name
- Status filtering (pass/fail based on tolerance)
- Adapter prefix filtering
- Row limiting (top 100 vs all)
- CSV export (shown rows or all)
- Click-through to detailed layer view

**Performance:**
- Efficient filtering and sorting with useMemo
- Virtual scrolling ready
- Responsive table layout

**Usage:**
```tsx
<LayerComparisonTable
  divergences={layerDivergences}
  tolerance={1e-5}
  adapterFilter="adapter:lora"
  onLayerClick={(layer) => showDetail(layer)}
/>
```

### 3. StatisticalSummary.tsx

**Purpose:** Statistical analysis and distribution visualization

**Features:**
- Key metrics: mean, median, std dev, min, max, pass rate
- Distribution histogram (20 buckets)
- Outlier detection (>2σ from mean)
- Top outliers list with standard deviation indicators
- Percentage-based visualization

**Calculations:**
- Mean error across all layers
- Median error for central tendency
- Standard deviation for spread
- Pass rate (% within tolerance)
- Outlier identification

**Usage:**
```tsx
<StatisticalSummary
  divergences={layerDivergences}
  tolerance={1e-5}
/>
```

### 4. LayerDetailModal.tsx

**Purpose:** Detailed comparison view for individual layers

**Features:**
- Side-by-side golden vs current statistics
- Error summary with tolerance comparison
- Difference magnitude visualization
- Distribution comparison charts
- Full tensor statistics (L2, max, mean, element count)

**Visualizations:**
- Progress bars for delta magnitudes
- Color-coded pass/fail indicators
- Exponential notation for scientific precision

**Usage:**
```tsx
<LayerDetailModal
  layer={selectedLayer}
  tolerance={1e-5}
  open={isOpen}
  onOpenChange={setIsOpen}
/>
```

### 5. GoldenCompareModalEnhanced.tsx

**Purpose:** Integrated golden run comparison with all visualization modes

**Features:**
- Three view modes: Table, Heatmap, Statistics
- Full golden run configuration
- Adapter filtering across all views
- Verification status badges
- Export capabilities
- Error recovery integration

**View Modes:**
1. **Table View:** Traditional sortable table
2. **Heatmap View:** Visual grid of all layers
3. **Statistics View:** Statistical analysis and distribution

**Usage:**
```tsx
<GoldenCompareModalEnhanced
  open={isOpen}
  onOpenChange={setIsOpen}
  bundleId="bundle-123"
/>
```

## Data Structures

### LayerDivergence
```typescript
interface LayerDivergence {
  layer_id: string;
  golden: EpsilonStats;
  current: EpsilonStats;
  relative_error: number;
}
```

### EpsilonStats
```typescript
interface EpsilonStats {
  l2_error: number;
  max_error: number;
  mean_error: number;
  element_count: number;
}
```

## Visualization Approach

### Heatmap Color Scale
- **Green (#10b981):** < 10% of tolerance (low error)
- **Lime (#84cc16):** 10-30% of tolerance
- **Yellow (#eab308):** 30-50% of tolerance
- **Orange (#f97316):** 50-70% of tolerance
- **Red (#ef4444):** 70-90% of tolerance
- **Dark Red (#991b1b):** > 90% of tolerance (critical)

### Statistical Analysis
1. **Mean Error:** Average relative error across all layers
2. **Median Error:** Central tendency (robust to outliers)
3. **Standard Deviation:** Measure of error spread
4. **Outliers:** Layers with error > mean + 2σ
5. **Pass Rate:** Percentage of layers within tolerance

### Performance Considerations

**Optimization Techniques:**
1. **useMemo for filtering/sorting:** Prevents unnecessary recalculations
2. **Grid virtualization:** Handles 100+ layers efficiently
3. **Lazy loading:** Detail modal only renders when opened
4. **CSV export:** Streaming export for large datasets

**Responsive Design:**
- Mobile-friendly grid layout
- Adaptive column widths
- Touch-friendly controls
- Accessible color contrast

## Integration Points

### With GoldenCompareModal
- Replace existing table view with enhanced version
- Add view mode selector (table/heatmap/stats)
- Integrate layer detail modal
- Maintain existing filtering logic

### With API
```typescript
// Fetch golden run comparison
const result = await apiClient.goldenCompare({
  golden: 'baseline-v1',
  bundle_id: 'bundle-123',
  strictness: 'epsilon-tolerant',
  epsilon_tolerance: 1e-5,
  verify_toolchain: true,
  verify_adapters: true,
  verify_device: false,
  verify_signature: true,
});

// Extract divergences
const divergences = result.epsilon_comparison.divergent_layers;
const tolerance = result.epsilon_comparison.tolerance;
```

### With Existing Components
```typescript
import {
  EpsilonHeatmap,
  LayerComparisonTable,
  StatisticalSummary,
  LayerDetailModal,
} from './components/golden';
```

## Success Criteria

- [x] Clear heatmap visualization with color-coded layers
- [x] Interactive layer details on click
- [x] Statistical summary with distribution histogram
- [x] Performance optimized for 100+ layers
- [x] Full integration with GoldenCompareModal
- [x] Export capabilities for analysis
- [x] Responsive design for all screen sizes
- [x] Accessible UI with proper ARIA labels

## File Locations

```
/Users/star/Dev/aos/ui/src/components/golden/
├── EpsilonHeatmap.tsx              # Heatmap visualization
├── LayerComparisonTable.tsx        # Sortable table view
├── StatisticalSummary.tsx          # Statistical analysis
├── LayerDetailModal.tsx            # Detailed layer view
├── GoldenCompareModalEnhanced.tsx  # Integrated modal
├── index.ts                        # Barrel exports
└── EPSILON_HEATMAP_README.md       # This file
```

## Future Enhancements

1. **Historical Trending:** Track layer divergences over multiple runs
2. **Anomaly Detection:** ML-based outlier identification
3. **Layer Grouping:** Custom layer groupings beyond adapter prefix
4. **Export Formats:** PDF reports, JSON exports
5. **Diff Visualization:** Tensor value diff display
6. **Performance Profiling:** Layer-by-layer performance metrics
7. **Comparative Analysis:** Multi-baseline comparison
8. **Alert Thresholds:** Configurable tolerance per layer

## References

- API Types: `/Users/star/Dev/aos/ui/src/api/types.ts`
- Original Modal: `/Users/star/Dev/aos/ui/src/components/GoldenCompareModal.tsx`
- Golden Runs Page: `/Users/star/Dev/aos/ui/src/components/GoldenRuns.tsx`
- CLAUDE.md: Architecture patterns and best practices
