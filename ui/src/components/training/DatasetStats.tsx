import React, { useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell } from 'recharts';
import { FileText, Hash, Languages, TrendingUp, Database } from 'lucide-react';
import { formatBytes, formatNumber } from '@/lib/formatters';

interface DatasetStatistics {
  num_examples: number;
  avg_input_length: number;
  avg_target_length: number;
  language_distribution?: Record<string, number>;
  file_type_distribution?: Record<string, number>;
  total_tokens: number;
  computed_at?: string;
}

interface DatasetFile {
  id: string;
  file_name: string;
  size_bytes: number;
  hash_b3: string;
  mime_type?: string;
}

interface DatasetStatsProps {
  datasetId?: string;
  fileCount: number;
  totalSizeBytes: number;
  files?: DatasetFile[];
  statistics?: DatasetStatistics;
  loading?: boolean;
}

const CHART_COLORS = [
  '#8b5cf6', '#6366f1', '#3b82f6', '#0ea5e9', '#06b6d4',
  '#14b8a6', '#10b981', '#84cc16', '#eab308', '#f59e0b'
];

const DatasetStatsComponent: React.FC<DatasetStatsProps> = ({
  datasetId,
  fileCount,
  totalSizeBytes,
  files = [],
  statistics,
  loading = false,
}) => {
  // Parse language distribution for chart
  const languageData = useMemo(() => {
    if (!statistics?.language_distribution) return [];

    const dist = typeof statistics.language_distribution === 'string'
      ? JSON.parse(statistics.language_distribution)
      : statistics.language_distribution;

    return Object.entries(dist)
      .map(([name, count]) => ({ name, count: count as number }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 10); // Top 10 languages
  }, [statistics?.language_distribution]);

  // Parse file type distribution for chart
  const fileTypeData = useMemo(() => {
    if (!statistics?.file_type_distribution) return [];

    const dist = typeof statistics.file_type_distribution === 'string'
      ? JSON.parse(statistics.file_type_distribution)
      : statistics.file_type_distribution;

    return Object.entries(dist)
      .map(([name, count]) => ({ name, count: count as number }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 10); // Top 10 file types
  }, [statistics?.file_type_distribution]);

  // Token distribution histogram (binned by size ranges)
  const tokenDistribution = useMemo(() => {
    if (!files || files.length === 0) return [];

    // Create bins: 0-100, 100-500, 500-1k, 1k-5k, 5k-10k, 10k+
    const bins = [
      { range: '0-100', min: 0, max: 100, count: 0 },
      { range: '100-500', min: 100, max: 500, count: 0 },
      { range: '500-1k', min: 500, max: 1000, count: 0 },
      { range: '1k-5k', min: 1000, max: 5000, count: 0 },
      { range: '5k-10k', min: 5000, max: 10000, count: 0 },
      { range: '10k+', min: 10000, max: Infinity, count: 0 },
    ];

    // Estimate tokens as size_bytes / 4 (rough approximation)
    files.forEach(file => {
      const estimatedTokens = file.size_bytes / 4;
      const bin = bins.find(b => estimatedTokens >= b.min && estimatedTokens < b.max);
      if (bin) bin.count++;
    });

    return bins.filter(b => b.count > 0);
  }, [files]);

  const avgTokensPerDoc = statistics
    ? Math.round(statistics.total_tokens / (statistics.num_examples || 1))
    : 0;

  if (loading) {
    return (
      <div className="space-y-4 animate-pulse">
        <Card>
          <CardHeader>
            <div className="h-6 bg-muted rounded w-1/3" />
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              <div className="h-4 bg-muted rounded w-full" />
              <div className="h-4 bg-muted rounded w-2/3" />
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Overview Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Files</CardTitle>
            <FileText className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{formatNumber(fileCount)}</div>
            <p className="text-xs text-muted-foreground">
              {formatBytes(totalSizeBytes)}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Tokens</CardTitle>
            <Hash className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {statistics ? formatNumber(statistics.total_tokens) : '—'}
            </div>
            <p className="text-xs text-muted-foreground">
              Across all documents
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Avg Tokens/Doc</CardTitle>
            <TrendingUp className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {statistics ? formatNumber(avgTokensPerDoc) : '—'}
            </div>
            <p className="text-xs text-muted-foreground">
              Average per document
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Examples</CardTitle>
            <Database className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {statistics ? formatNumber(statistics.num_examples) : '—'}
            </div>
            <p className="text-xs text-muted-foreground">
              Training examples
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Language Distribution */}
      {languageData.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Languages className="h-5 w-5" />
              Language Distribution
            </CardTitle>
            <CardDescription>
              Files by programming language
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={languageData}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="name"
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <YAxis
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <Tooltip
                  contentStyle={{
                    backgroundColor: 'hsl(var(--background))',
                    border: '1px solid hsl(var(--border))',
                    borderRadius: '8px',
                  }}
                  labelStyle={{ color: 'hsl(var(--foreground))' }}
                />
                <Bar dataKey="count" radius={[8, 8, 0, 0]}>
                  {languageData.map((_, index) => (
                    <Cell key={`cell-${index}`} fill={CHART_COLORS[index % CHART_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
            <div className="flex flex-wrap gap-2 mt-4">
              {languageData.slice(0, 5).map((lang, idx) => (
                <Badge key={lang.name} variant="outline" className="gap-1">
                  <div
                    className="w-3 h-3 rounded-full"
                    style={{ backgroundColor: CHART_COLORS[idx % CHART_COLORS.length] }}
                  />
                  {lang.name}: {lang.count}
                </Badge>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* File Type Distribution */}
      {fileTypeData.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              File Type Distribution
            </CardTitle>
            <CardDescription>
              Files by extension
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={fileTypeData}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="name"
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <YAxis
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <Tooltip
                  contentStyle={{
                    backgroundColor: 'hsl(var(--background))',
                    border: '1px solid hsl(var(--border))',
                    borderRadius: '8px',
                  }}
                  labelStyle={{ color: 'hsl(var(--foreground))' }}
                />
                <Bar dataKey="count" radius={[8, 8, 0, 0]}>
                  {fileTypeData.map((_, index) => (
                    <Cell key={`cell-${index}`} fill={CHART_COLORS[index % CHART_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      )}

      {/* Token Distribution Histogram */}
      {tokenDistribution.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Hash className="h-5 w-5" />
              Token Distribution
            </CardTitle>
            <CardDescription>
              Documents by estimated token count
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ResponsiveContainer width="100%" height={300}>
              <BarChart data={tokenDistribution}>
                <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                <XAxis
                  dataKey="range"
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <YAxis
                  className="text-xs"
                  tick={{ fill: 'currentColor' }}
                />
                <Tooltip
                  contentStyle={{
                    backgroundColor: 'hsl(var(--background))',
                    border: '1px solid hsl(var(--border))',
                    borderRadius: '8px',
                  }}
                  labelStyle={{ color: 'hsl(var(--foreground))' }}
                />
                <Bar dataKey="count" fill="#8b5cf6" radius={[8, 8, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      )}

      {/* Quality Metrics */}
      {statistics && (
        <Card>
          <CardHeader>
            <CardTitle>Dataset Quality Metrics</CardTitle>
            <CardDescription>
              Average lengths and statistics
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <div className="text-sm font-medium text-muted-foreground mb-1">
                  Average Input Length
                </div>
                <div className="text-2xl font-bold">
                  {Math.round(statistics.avg_input_length)} tokens
                </div>
              </div>
              <div>
                <div className="text-sm font-medium text-muted-foreground mb-1">
                  Average Target Length
                </div>
                <div className="text-2xl font-bold">
                  {Math.round(statistics.avg_target_length)} tokens
                </div>
              </div>
            </div>
            {statistics.computed_at && (
              <div className="text-xs text-muted-foreground pt-2 border-t">
                Statistics computed: {new Date(statistics.computed_at).toLocaleString()}
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
};

export const DatasetStats = React.memo(DatasetStatsComponent);
export default DatasetStats;
