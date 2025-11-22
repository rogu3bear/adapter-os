import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import type { AdapterStack } from '@/api/types';
import { Layers, Calendar } from 'lucide-react';

interface StackDetailModalProps {
  stack: AdapterStack;
  open: boolean;
  onClose: () => void;
}

export function StackDetailModal({ stack, open, onClose }: StackDetailModalProps) {
  return (
    <Dialog open={open} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Adapter Stack: {stack.name}</DialogTitle>
          <DialogDescription>
            Stack ID: <span className="font-mono">{stack.id}</span>
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>General Information</CardTitle>
            </CardHeader>
            <CardContent className="grid gap-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Created</p>
                  <p className="text-sm mt-1">
                    {new Date(stack.created_at).toLocaleString()}
                  </p>
                </div>
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Last Updated</p>
                  <p className="text-sm mt-1">
                    {new Date(stack.updated_at).toLocaleString()}
                  </p>
                </div>
              </div>

              {stack.description && (
                <div>
                  <p className="text-sm font-medium text-muted-foreground">Description</p>
                  <p className="text-sm mt-1 text-foreground">{stack.description}</p>
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Layers className="h-5 w-5" />
                Adapters ({stack.adapters.length})
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                {stack.adapters.map((adapter, index) => {
                  const adapterId =
                    typeof adapter === 'string' ? adapter : adapter.adapter_id;
                  const gate =
                    typeof adapter === 'object' && 'gate' in adapter
                      ? adapter.gate
                      : undefined;

                  return (
                    <div
                      key={index}
                      className="flex items-center justify-between p-3 border rounded-lg"
                    >
                      <div className="flex items-center gap-3">
                        <Badge variant="outline" className="font-mono">
                          {index + 1}
                        </Badge>
                        <span className="font-medium">{adapterId}</span>
                      </div>
                      {gate !== undefined && (
                        <div className="flex items-center gap-2">
                          <span className="text-sm text-muted-foreground">Gate:</span>
                          <Badge variant="secondary">{gate}</Badge>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </CardContent>
          </Card>
        </div>
      </DialogContent>
    </Dialog>
  );
}
