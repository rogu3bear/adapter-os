/**
 * Display and Formatting Types
 * Types for formatting data for display
 */

export type DateFormat =
  | 'short'          // 12/19/25
  | 'medium'         // Dec 19, 2025
  | 'long'           // December 19, 2025
  | 'full'           // Thursday, December 19, 2025
  | 'relative'       // 2 hours ago
  | 'iso'            // 2025-12-19T00:00:00Z
  | 'time';          // 3:45 PM

export type NumberFormat =
  | 'decimal'        // 1,234.56
  | 'percent'        // 12.34%
  | 'currency'       // $1,234.56
  | 'compact'        // 1.2K
  | 'scientific';    // 1.23e+3

export type ByteFormat =
  | 'B'              // Bytes
  | 'KB'             // Kilobytes
  | 'MB'             // Megabytes
  | 'GB'             // Gigabytes
  | 'TB'             // Terabytes
  | 'auto';          // Auto-select best unit

export interface FormatDateOptions {
  format?: DateFormat;
  locale?: string;
  timezone?: string;
}

export interface FormatNumberOptions {
  format?: NumberFormat;
  decimals?: number;
  locale?: string;
  currency?: string;
}

export interface FormatBytesOptions {
  unit?: ByteFormat;
  decimals?: number;
  binary?: boolean; // Use 1024 instead of 1000
}

export interface FormatDurationOptions {
  format?: 'short' | 'long' | 'verbose';
  units?: ('days' | 'hours' | 'minutes' | 'seconds' | 'milliseconds')[];
  maxUnits?: number;
}

export interface DisplayValue {
  raw: any;
  formatted: string;
  type: 'text' | 'number' | 'date' | 'boolean' | 'bytes' | 'duration' | 'custom';
}

export interface ColorScheme {
  primary: string;
  secondary: string;
  success: string;
  warning: string;
  error: string;
  info: string;
  neutral: string;
}

export interface StatusDisplay {
  label: string;
  color: keyof ColorScheme;
  icon?: React.ReactNode;
  description?: string;
}

export interface MetricDisplay {
  label: string;
  value: DisplayValue;
  change?: {
    value: number;
    direction: 'up' | 'down' | 'neutral';
    period?: string;
  };
  trend?: number[];
  unit?: string;
  icon?: React.ReactNode;
}

export interface ChartDataPoint {
  x: number | string | Date;
  y: number;
  label?: string;
  color?: string;
}

export interface ChartSeries {
  name: string;
  data: ChartDataPoint[];
  color?: string;
  type?: 'line' | 'bar' | 'area' | 'scatter';
}

export interface TableCellDisplay {
  content: React.ReactNode;
  align?: 'left' | 'center' | 'right';
  className?: string;
  sortValue?: any;
}
