import React, { useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { LayerDivergence } from '../../api/types';

interface StatisticalSummaryProps {
  divergences: LayerDivergence[];
  tolerance: number;
}

export function StatisticalSummary({ divergences, tolerance }: StatisticalSummaryProps) {
  const stats = useMemo(() => {
    if (divergences.length === 0) {
      return {
        mean: 0,
        median: 0,
        stdDev: 0,
        min: 0,
        max: 0,
        passRate: 0,
        outliers: [],
        histogram: [],
      };
    }

    const errors = divergences.map(d => d.relative_error).sort((a, b) => a - b);
    const mean = errors.reduce((sum, e) => sum + e, 0) / errors.length;
    const median = errors[Math.floor(errors.length / 2)];
    const variance = errors.reduce((sum, e) => sum + Math.pow(e - mean, 2), 0) / errors.length;
    const stdDev = Math.sqrt(variance);
    const min = errors[0];
    const max = errors[errors.length - 1];
    const passRate = (divergences.filter(d => d.relative_error <= tolerance).length / divergences.length) * 100;

    // Identify outliers (>2 standard deviations from mean)
    const outlierThreshold = mean + 2 * stdDev;
    const outliers = divergences
      .filter(d => d.relative_error > outlierThreshold)
      .sort((a, b) => b.relative_error - a.relative_error)
      .slice(0, 10);

    // Create histogram buckets
    const bucketCount = 20;
    const bucketSize = (max - min) / bucketCount;
    const histogram = Array.from({ length: bucketCount }, (_, i) => {
      const bucketMin = min + i * bucketSize;
      const bucketMax = bucketMin + bucketSize;
      const count = errors.filter(e => e >= bucketMin && (i === bucketCount - 1 ? e <= bucketMax : e < bucketMax)).length;
      return {
        bucketMin,
        bucketMax,
        count,
        percentage: (count / errors.length) * 100,
      };
    });

    return { mean, median, stdDev, min, max, passRate, outliers, histogram };
  }, [divergences, tolerance]);

  const maxHistogramCount = Math.max(...stats.histogram.map(h => h.count), 1);

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>Statistical Summary</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Key metrics */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <div>
              <div className="text-sm text-muted-foreground">Mean Error</div>
              <div className="text-lg font-semibold font-mono">
                {stats.mean.toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Median Error</div>
              <div className="text-lg font-semibold font-mono">
                {stats.median.toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Std Deviation</div>
              <div className="text-lg font-semibold font-mono">
                {stats.stdDev.toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Pass Rate</div>
              <div className="text-lg font-semibold">
                {stats.passRate.toFixed(1)}%
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Min Error</div>
              <div className="text-lg font-semibold font-mono">
                {stats.min.toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Max Error</div>
              <div className="text-lg font-semibold font-mono">
                {stats.max.toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Total Layers</div>
              <div className="text-lg font-semibold">
                {divergences.length.toLocaleString()}
              </div>
            </div>
            <div>
              <div className="text-sm text-muted-foreground">Outliers</div>
              <div className="text-lg font-semibold">
                {stats.outliers.length}
              </div>
            </div>
          </div>

          {/* Distribution histogram */}
          <div>
            <div className="text-sm font-medium mb-2">Error Distribution</div>
            <div className="space-y-1">
              {stats.histogram.map((bucket, idx) => (
                <div key={idx} className="flex items-center gap-2">
                  <div className="text-xs font-mono text-muted-foreground w-32 text-right">
                    {bucket.bucketMin.toExponential(1)} - {bucket.bucketMax.toExponential(1)}
                  </div>
                  <div className="flex-1 flex items-center gap-2">
                    <div
                      className="bg-blue-500 rounded h-6 transition-all"
                      style={{
                        width: `${(bucket.count / maxHistogramCount) * 100}%`,
                        minWidth: bucket.count > 0 ? '2px' : '0',
                      }}
                    />
                    <div className="text-xs text-muted-foreground w-16">
                      {bucket.count} ({bucket.percentage.toFixed(1)}%)
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Outliers */}
          {stats.outliers.length > 0 && (
            <div>
              <div className="text-sm font-medium mb-2">
                Top Outliers (&gt;2σ from mean)
              </div>
              <div className="space-y-2">
                {stats.outliers.map(layer => (
                  <div
                    key={layer.layer_id}
                    className="flex items-center justify-between p-2 border rounded text-sm"
                  >
                    <div className="font-mono text-xs truncate flex-1 mr-4" title={layer.layer_id}>
                      {layer.layer_id}
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge variant="destructive">
                        {layer.relative_error.toExponential(2)}
                      </Badge>
                      <span className="text-xs text-muted-foreground">
                        ({((layer.relative_error - stats.mean) / stats.stdDev).toFixed(1)}σ)
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
