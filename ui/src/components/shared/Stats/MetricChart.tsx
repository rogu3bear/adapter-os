import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../ui/utils";

const metricChartVariants = cva(
  "relative",
  {
    variants: {
      size: {
        xs: "h-6 w-16",
        sm: "h-8 w-20",
        md: "h-10 w-24",
        lg: "h-12 w-32",
        xl: "h-16 w-40",
      },
    },
    defaultVariants: {
      size: "md",
    },
  }
);

export interface MetricChartProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof metricChartVariants> {
  /** Array of data points (0-100 normalized or raw values) */
  data: number[];
  /** Normalize data to 0-100 range */
  normalize?: boolean;
  /** Line color */
  color?: "default" | "success" | "warning" | "error" | "info";
  /** Fill area under the line */
  fill?: boolean;
  /** Show dots at data points */
  showDots?: boolean;
  /** Stroke width */
  strokeWidth?: number;
  /** Accessible label */
  label?: string;
}

const colorMap = {
  default: { stroke: "#6366f1", fill: "rgba(99, 102, 241, 0.1)" },
  success: { stroke: "#10b981", fill: "rgba(16, 185, 129, 0.1)" },
  warning: { stroke: "#f59e0b", fill: "rgba(245, 158, 11, 0.1)" },
  error: { stroke: "#ef4444", fill: "rgba(239, 68, 68, 0.1)" },
  info: { stroke: "#3b82f6", fill: "rgba(59, 130, 246, 0.1)" },
};

/**
 * Simple inline sparkline chart for displaying metric trends.
 */
export function MetricChart({
  className,
  size,
  data,
  normalize = true,
  color = "default",
  fill = true,
  showDots = false,
  strokeWidth = 1.5,
  label = "Metric trend",
  ...props
}: MetricChartProps) {
  if (data.length < 2) {
    return (
      <div
        className={cn(metricChartVariants({ size }), className)}
        aria-label={label}
        {...props}
      />
    );
  }

  // Normalize data if needed
  const min = normalize ? Math.min(...data) : 0;
  const max = normalize ? Math.max(...data) : 100;
  const range = max - min || 1;
  const normalizedData = data.map((v) => ((v - min) / range) * 100);

  // Generate SVG path
  const width = 100;
  const height = 100;
  const padding = 2;
  const effectiveWidth = width - padding * 2;
  const effectiveHeight = height - padding * 2;

  const points = normalizedData.map((value, index) => ({
    x: padding + (index / (normalizedData.length - 1)) * effectiveWidth,
    y: padding + effectiveHeight - (value / 100) * effectiveHeight,
  }));

  // Create smooth path using bezier curves
  const pathData = points.reduce((acc, point, index, arr) => {
    if (index === 0) {
      return `M ${point.x} ${point.y}`;
    }

    const prev = arr[index - 1];
    const tension = 0.3;
    const cp1x = prev.x + (point.x - prev.x) * tension;
    const cp1y = prev.y;
    const cp2x = point.x - (point.x - prev.x) * tension;
    const cp2y = point.y;

    return `${acc} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${point.x} ${point.y}`;
  }, "");

  // Create fill path
  const fillPath = fill
    ? `${pathData} L ${points[points.length - 1].x} ${height - padding} L ${padding} ${height - padding} Z`
    : "";

  const colors = colorMap[color];

  return (
    <div
      className={cn(metricChartVariants({ size }), className)}
      aria-label={label}
      role="img"
      {...props}
    >
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className="h-full w-full"
      >
        {fill && (
          <path
            d={fillPath}
            fill={colors.fill}
            className="transition-opacity"
          />
        )}
        <path
          d={pathData}
          fill="none"
          stroke={colors.stroke}
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeLinejoin="round"
          className="transition-all"
        />
        {showDots &&
          points.map((point, index) => (
            <circle
              key={index}
              cx={point.x}
              cy={point.y}
              r={2}
              fill={colors.stroke}
            />
          ))}
      </svg>
    </div>
  );
}

export interface BarChartProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof metricChartVariants> {
  /** Array of data points */
  data: number[];
  /** Bar color */
  color?: "default" | "success" | "warning" | "error" | "info";
  /** Gap between bars */
  gap?: number;
  /** Accessible label */
  label?: string;
}

/**
 * Simple inline bar chart for displaying metric values.
 */
export function BarChart({
  className,
  size,
  data,
  color = "default",
  gap = 2,
  label = "Bar chart",
  ...props
}: BarChartProps) {
  if (data.length === 0) {
    return (
      <div
        className={cn(metricChartVariants({ size }), className)}
        aria-label={label}
        {...props}
      />
    );
  }

  const max = Math.max(...data, 1);
  const normalizedData = data.map((v) => (v / max) * 100);
  const colors = colorMap[color];

  return (
    <div
      className={cn(metricChartVariants({ size }), "flex items-end", className)}
      aria-label={label}
      role="img"
      style={{ gap: `${gap}px` }}
      {...props}
    >
      {normalizedData.map((value, index) => (
        <div
          key={index}
          className="flex-1 rounded-t-sm transition-all"
          style={{
            height: `${Math.max(value, 2)}%`,
            backgroundColor: colors.stroke,
            opacity: 0.6 + (value / 100) * 0.4,
          }}
          title={`${data[index]}`}
        />
      ))}
    </div>
  );
}

export interface AreaChartProps extends MetricChartProps {
  /** Multiple data series */
  series?: Array<{
    data: number[];
    color?: "default" | "success" | "warning" | "error" | "info";
  }>;
}

/**
 * Stacked area chart for multiple data series.
 */
export function AreaChart({
  className,
  size,
  data,
  series,
  normalize = true,
  color = "default",
  label = "Area chart",
  ...props
}: AreaChartProps) {
  const allSeries = series ?? [{ data, color }];

  if (allSeries.every((s) => s.data.length < 2)) {
    return (
      <div
        className={cn(metricChartVariants({ size }), className)}
        aria-label={label}
        {...props}
      />
    );
  }

  const allData = allSeries.flatMap((s) => s.data);
  const min = normalize ? Math.min(...allData) : 0;
  const max = normalize ? Math.max(...allData) : 100;
  const range = max - min || 1;

  const width = 100;
  const height = 100;
  const padding = 2;
  const effectiveWidth = width - padding * 2;
  const effectiveHeight = height - padding * 2;

  return (
    <div
      className={cn(metricChartVariants({ size }), className)}
      aria-label={label}
      role="img"
      {...props}
    >
      <svg
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className="h-full w-full"
      >
        {allSeries.map((seriesItem, seriesIndex) => {
          const normalizedData = seriesItem.data.map(
            (v) => ((v - min) / range) * 100
          );
          const points = normalizedData.map((value, index) => ({
            x: padding + (index / (normalizedData.length - 1)) * effectiveWidth,
            y:
              padding + effectiveHeight - (value / 100) * effectiveHeight,
          }));

          const pathData = points.reduce((acc, point, index) => {
            if (index === 0) return `M ${point.x} ${point.y}`;
            return `${acc} L ${point.x} ${point.y}`;
          }, "");

          const fillPath = `${pathData} L ${points[points.length - 1].x} ${height - padding} L ${padding} ${height - padding} Z`;
          const colors = colorMap[seriesItem.color ?? "default"];

          return (
            <g key={seriesIndex}>
              <path
                d={fillPath}
                fill={colors.fill}
                opacity={0.5 + seriesIndex * 0.2}
              />
              <path
                d={pathData}
                fill="none"
                stroke={colors.stroke}
                strokeWidth={1.5}
                strokeLinecap="round"
              />
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export { metricChartVariants };
