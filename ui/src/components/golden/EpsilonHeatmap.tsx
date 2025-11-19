import React, { useMemo, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { LayerDivergence } from '../../api/types';
import { LayerDetailModal } from './LayerDetailModal';

interface EpsilonHeatmapProps {
  divergences: LayerDivergence[];
  tolerance: number;
  adapterFilter?: string | null;
  onAdapterClick?: (adapter: string) => void;
}

export function EpsilonHeatmap({
  divergences,
  tolerance,
  adapterFilter,
  onAdapterClick,
}: EpsilonHeatmapProps) {
  const [selectedLayer, setSelectedLayer] = useState<LayerDivergence | null>(null);
  const [zoomLevel, setZoomLevel] = useState<number>(1);
  const [panOffset, setPanOffset] = useState<{ x: number; y: number }>({ x: 0, y: 0 });

  // Group divergences by adapter prefix
  const groupedLayers = useMemo(() => {
    const groups = new Map<string, LayerDivergence[]>();

    divergences.forEach(div => {
      const prefix = div.layer_id.startsWith('adapter:')
        ? div.layer_id.split('/')[0]
        : 'base_model';

      if (!groups.has(prefix)) {
        groups.set(prefix, []);
      }
      groups.get(prefix)!.push(div);
    });

    return Array.from(groups.entries()).map(([prefix, layers]) => ({
      prefix,
      layers: layers.sort((a, b) => b.relative_error - a.relative_error),
    }));
  }, [divergences]);

  // Calculate color based on relative error
  const getErrorColor = (relativeError: number): string => {
    const normalized = Math.min(relativeError / tolerance, 1);

    if (normalized < 0.1) return '#10b981'; // green-500
    if (normalized < 0.3) return '#84cc16'; // lime-500
    if (normalized < 0.5) return '#eab308'; // yellow-500
    if (normalized < 0.7) return '#f97316'; // orange-500
    if (normalized < 0.9) return '#ef4444'; // red-500
    return '#991b1b'; // red-900
  };

  // Calculate cell size based on zoom
  const cellSize = Math.max(8, Math.min(40, 20 * zoomLevel));
  const cellGap = Math.max(1, 2 * zoomLevel);

  const handleZoomIn = () => setZoomLevel(prev => Math.min(prev + 0.25, 3));
  const handleZoomOut = () => setZoomLevel(prev => Math.max(prev - 0.25, 0.5));
  const handleResetZoom = () => {
    setZoomLevel(1);
    setPanOffset({ x: 0, y: 0 });
  };

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>Layer Divergence Heatmap</CardTitle>
            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" onClick={handleZoomOut} disabled={zoomLevel <= 0.5}>
                -
              </Button>
              <span className="text-sm text-muted-foreground">{Math.round(zoomLevel * 100)}%</span>
              <Button variant="outline" size="sm" onClick={handleZoomIn} disabled={zoomLevel >= 3}>
                +
              </Button>
              <Button variant="outline" size="sm" onClick={handleResetZoom}>
                Reset
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {/* Color scale legend */}
          <div className="mb-4 flex items-center gap-2">
            <span className="text-sm text-muted-foreground">Error Scale:</span>
            <div className="flex items-center gap-1">
              <div className="w-4 h-4 rounded" style={{ backgroundColor: '#10b981' }} />
              <span className="text-xs">Low</span>
            </div>
            <div className="flex items-center gap-1">
              <div className="w-4 h-4 rounded" style={{ backgroundColor: '#eab308' }} />
              <span className="text-xs">Medium</span>
            </div>
            <div className="flex items-center gap-1">
              <div className="w-4 h-4 rounded" style={{ backgroundColor: '#ef4444' }} />
              <span className="text-xs">High</span>
            </div>
            <div className="flex items-center gap-1">
              <div className="w-4 h-4 rounded" style={{ backgroundColor: '#991b1b' }} />
              <span className="text-xs">Critical</span>
            </div>
          </div>

          {/* Heatmap grid */}
          <div className="overflow-auto border rounded-lg p-4" style={{ maxHeight: '600px' }}>
            <div
              className="space-y-4"
              style={{
                transform: `scale(${zoomLevel}) translate(${panOffset.x}px, ${panOffset.y}px)`,
                transformOrigin: 'top left',
                transition: 'transform 0.2s',
              }}
            >
              {groupedLayers.map(({ prefix, layers }) => (
                <div key={prefix} className="space-y-2">
                  <div className="flex items-center gap-2">
                    <Badge
                      variant={adapterFilter === prefix ? 'default' : 'outline'}
                      className="cursor-pointer"
                      onClick={() => onAdapterClick?.(prefix)}
                    >
                      {prefix}
                    </Badge>
                    <span className="text-xs text-muted-foreground">
                      {layers.length} layers
                    </span>
                  </div>
                  <div
                    className="grid"
                    style={{
                      gridTemplateColumns: `repeat(auto-fill, ${cellSize}px)`,
                      gap: `${cellGap}px`,
                    }}
                  >
                    {layers.map((layer, idx) => (
                      <div
                        key={layer.layer_id}
                        className="rounded cursor-pointer hover:ring-2 hover:ring-blue-500 transition-all"
                        style={{
                          width: `${cellSize}px`,
                          height: `${cellSize}px`,
                          backgroundColor: getErrorColor(layer.relative_error),
                        }}
                        onClick={() => setSelectedLayer(layer)}
                        title={`${layer.layer_id}\nRelative Error: ${layer.relative_error.toExponential(2)}`}
                      />
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Statistics summary */}
          <div className="mt-4 grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
            <div>
              <div className="text-muted-foreground">Total Layers</div>
              <div className="text-lg font-semibold">{divergences.length.toLocaleString()}</div>
            </div>
            <div>
              <div className="text-muted-foreground">Max Error</div>
              <div className="text-lg font-semibold">
                {Math.max(...divergences.map(d => d.relative_error)).toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-muted-foreground">Mean Error</div>
              <div className="text-lg font-semibold">
                {(divergences.reduce((sum, d) => sum + d.relative_error, 0) / divergences.length).toExponential(2)}
              </div>
            </div>
            <div>
              <div className="text-muted-foreground">Within Tolerance</div>
              <div className="text-lg font-semibold">
                {((divergences.filter(d => d.relative_error <= tolerance).length / divergences.length) * 100).toFixed(1)}%
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {selectedLayer && (
        <LayerDetailModal
          layer={selectedLayer}
          tolerance={tolerance}
          open={!!selectedLayer}
          onOpenChange={(open) => !open && setSelectedLayer(null)}
        />
      )}
    </div>
  );
}
