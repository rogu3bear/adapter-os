/**
 * Chart Colors - Design System Integration
 *
 * Uses CSS custom properties from design-system.css for consistent,
 * theme-aware chart colors across the application.
 *
 * CSS Variables:
 * --chart-1: 222.2 47.4% 11.2%  (Primary - Dark blue-grey)
 * --chart-2: 142 76% 36%         (Success - Green)
 * --chart-3: 45 93% 47%          (Warning - Amber)
 * --chart-4: 0 72% 51%           (Error - Red)
 * --chart-5: 199 89% 48%         (Info - Blue)
 */

export const CHART_COLORS = {
  primary: 'hsl(var(--chart-1))',
  secondary: 'hsl(var(--chart-5))',
  success: 'hsl(var(--chart-2))',
  warning: 'hsl(var(--chart-3))',
  error: 'hsl(var(--chart-4))',
  info: 'hsl(var(--chart-5))',
} as const;

/**
 * Color palette for multi-series charts
 * Uses a predefined sequence for consistent data visualization
 */
export const CHART_PALETTE = [
  CHART_COLORS.primary,
  CHART_COLORS.success,
  CHART_COLORS.warning,
  CHART_COLORS.error,
  CHART_COLORS.info,
] as const;

/**
 * Semantic color mappings for specific metrics
 */
export const METRIC_COLORS = {
  cpu: CHART_COLORS.primary,
  memory: CHART_COLORS.success,
  disk: CHART_COLORS.warning,
  gpu: CHART_COLORS.error,
  gpuMemory: CHART_COLORS.error, // GPU memory uses same color as GPU
  network: CHART_COLORS.info,

  // Training metrics
  trainingLoss: CHART_COLORS.primary,
  validationLoss: CHART_COLORS.error,
  learningRate: CHART_COLORS.success,
  gradientNorm: CHART_COLORS.warning,
  tokensPerSecond: CHART_COLORS.info,

  // Performance metrics
  latency: CHART_COLORS.error,
  throughput: CHART_COLORS.success,
  errorRate: CHART_COLORS.error,
  cacheHitRate: CHART_COLORS.success,
} as const;
