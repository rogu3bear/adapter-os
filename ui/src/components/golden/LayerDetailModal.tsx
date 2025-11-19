import React from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '../ui/dialog';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { LayerDivergence } from '../../api/types';

interface LayerDetailModalProps {
  layer: LayerDivergence;
  tolerance: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function LayerDetailModal({
  layer,
  tolerance,
  open,
  onOpenChange,
}: LayerDetailModalProps) {
  const passes = layer.relative_error <= tolerance;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span className="font-mono text-sm">{layer.layer_id}</span>
            <Badge variant={passes ? 'default' : 'destructive'}>
              {passes ? 'Pass' : 'Fail'}
            </Badge>
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {/* Relative Error */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Error Summary</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <div className="grid grid-cols-2 gap-4 text-sm">
                <div>
                  <div className="text-muted-foreground">Relative Error</div>
                  <div className="text-lg font-semibold font-mono">
                    {layer.relative_error.toExponential(4)}
                  </div>
                </div>
                <div>
                  <div className="text-muted-foreground">Tolerance</div>
                  <div className="text-lg font-semibold font-mono">
                    {tolerance.toExponential(4)}
                  </div>
                </div>
                <div>
                  <div className="text-muted-foreground">Margin</div>
                  <div className={`text-lg font-semibold font-mono ${passes ? 'text-green-600' : 'text-red-600'}`}>
                    {passes ? '-' : '+'}{Math.abs(layer.relative_error - tolerance).toExponential(4)}
                  </div>
                </div>
                <div>
                  <div className="text-muted-foreground">Ratio to Tolerance</div>
                  <div className="text-lg font-semibold font-mono">
                    {(layer.relative_error / tolerance).toFixed(2)}x
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Side-by-side comparison */}
          <div className="grid grid-cols-2 gap-4">
            {/* Golden statistics */}
            <Card>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  Golden Run
                  <Badge variant="outline">Baseline</Badge>
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">L2 Error</span>
                  <span className="font-mono">{layer.golden.l2_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Max Error</span>
                  <span className="font-mono">{layer.golden.max_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Mean Error</span>
                  <span className="font-mono">{layer.golden.mean_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Element Count</span>
                  <span className="font-mono">{layer.golden.element_count.toLocaleString()}</span>
                </div>
              </CardContent>
            </Card>

            {/* Current statistics */}
            <Card>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  Current Run
                  <Badge variant="outline">Test</Badge>
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-2 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">L2 Error</span>
                  <span className="font-mono">{layer.current.l2_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Max Error</span>
                  <span className="font-mono">{layer.current.max_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Mean Error</span>
                  <span className="font-mono">{layer.current.mean_error.toExponential(4)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Element Count</span>
                  <span className="font-mono">{layer.current.element_count.toLocaleString()}</span>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Difference visualization */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Difference Magnitude</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <div className="space-y-2 text-sm">
                <div>
                  <div className="flex justify-between mb-1">
                    <span className="text-muted-foreground">L2 Error Delta</span>
                    <span className="font-mono">
                      {Math.abs(layer.current.l2_error - layer.golden.l2_error).toExponential(4)}
                    </span>
                  </div>
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className="bg-blue-600 h-2 rounded-full"
                      style={{
                        width: `${Math.min((Math.abs(layer.current.l2_error - layer.golden.l2_error) / Math.max(layer.golden.l2_error, layer.current.l2_error)) * 100, 100)}%`,
                      }}
                    />
                  </div>
                </div>
                <div>
                  <div className="flex justify-between mb-1">
                    <span className="text-muted-foreground">Max Error Delta</span>
                    <span className="font-mono">
                      {Math.abs(layer.current.max_error - layer.golden.max_error).toExponential(4)}
                    </span>
                  </div>
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className="bg-orange-600 h-2 rounded-full"
                      style={{
                        width: `${Math.min((Math.abs(layer.current.max_error - layer.golden.max_error) / Math.max(layer.golden.max_error, layer.current.max_error)) * 100, 100)}%`,
                      }}
                    />
                  </div>
                </div>
                <div>
                  <div className="flex justify-between mb-1">
                    <span className="text-muted-foreground">Mean Error Delta</span>
                    <span className="font-mono">
                      {Math.abs(layer.current.mean_error - layer.golden.mean_error).toExponential(4)}
                    </span>
                  </div>
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className="bg-purple-600 h-2 rounded-full"
                      style={{
                        width: `${Math.min((Math.abs(layer.current.mean_error - layer.golden.mean_error) / Math.max(layer.golden.mean_error, layer.current.mean_error)) * 100, 100)}%`,
                      }}
                    />
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Value distribution comparison */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Distribution Comparison</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-3 gap-4 text-sm">
                <div className="text-center">
                  <div className="text-muted-foreground mb-1">Min (Golden)</div>
                  <div className="font-mono text-xs">
                    {(layer.golden.mean_error - layer.golden.max_error).toExponential(2)}
                  </div>
                </div>
                <div className="text-center">
                  <div className="text-muted-foreground mb-1">Mean</div>
                  <div className="font-mono text-xs">
                    Golden: {layer.golden.mean_error.toExponential(2)}<br />
                    Current: {layer.current.mean_error.toExponential(2)}
                  </div>
                </div>
                <div className="text-center">
                  <div className="text-muted-foreground mb-1">Max</div>
                  <div className="font-mono text-xs">
                    Golden: {layer.golden.max_error.toExponential(2)}<br />
                    Current: {layer.current.max_error.toExponential(2)}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
