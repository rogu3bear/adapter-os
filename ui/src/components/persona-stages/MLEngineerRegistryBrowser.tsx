import React, { useState, useMemo } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Search, Download, Eye, Clock, CheckCircle, XCircle, Loader2, AlertCircle } from 'lucide-react';
import { useAdapters } from '@/pages/Adapters/useAdapters';
import type { Adapter } from '@/api/adapter-types';

interface AdapterEntry {
  id: string;
  name: string;
  version: string;
  status: 'active' | 'training' | 'deprecated';
  baseModel: string;
  performance: number;
  createdAt: string;
  size: string;
}

// Transform API adapter to UI adapter entry
function transformAdapter(adapter: Adapter): AdapterEntry {
  // Determine status based on current_state
  let status: 'active' | 'training' | 'deprecated' = 'deprecated';
  if (adapter.current_state === 'hot' || adapter.current_state === 'warm' || adapter.current_state === 'resident') {
    status = 'active';
  } else if (adapter.current_state === 'cold' || adapter.current_state === 'unloaded') {
    status = 'deprecated';
  }

  // Calculate size from memory_bytes
  const size = adapter.memory_bytes
    ? adapter.memory_bytes > 1024 * 1024 * 1024
      ? `${(adapter.memory_bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`
      : `${(adapter.memory_bytes / (1024 * 1024)).toFixed(0)}MB`
    : 'Unknown';

  // Use activation_count as a proxy for performance (higher is better)
  const performance = adapter.activation_count
    ? Math.min(100, Math.round((adapter.activation_count / 100) * 100))
    : 0;

  return {
    id: adapter.adapter_id,
    name: adapter.name || adapter.adapter_id,
    version: adapter.version || adapter.revision || '1.0.0',
    status,
    baseModel: adapter.framework || 'Unknown',
    performance,
    createdAt: new Date(adapter.created_at).toISOString().split('T')[0],
    size,
  };
}

export default function MLEngineerRegistryBrowser() {
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedAdapter, setSelectedAdapter] = useState<string | null>(null);

  // Fetch adapters using the hook
  const { data, isLoading, isError, error } = useAdapters();

  // Transform and filter adapters
  const filteredAdapters = useMemo(() => {
    if (!data?.adapters) return [];

    const transformed = data.adapters.map(transformAdapter);

    if (!searchTerm) return transformed;

    return transformed.filter(adapter =>
      adapter.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      adapter.baseModel.toLowerCase().includes(searchTerm.toLowerCase())
    );
  }, [data?.adapters, searchTerm]);

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'active': return <CheckCircle className="h-4 w-4 text-green-500" />;
      case 'training': return <Clock className="h-4 w-4 text-yellow-500" />;
      case 'deprecated': return <XCircle className="h-4 w-4 text-red-500" />;
      default: return null;
    }
  };

  const getStatusBadge = (status: string) => {
    const variants = {
      active: 'default' as const,
      training: 'secondary' as const,
      deprecated: 'destructive' as const
    };
    return <Badge variant={variants[status as keyof typeof variants] || 'outline'}>{status}</Badge>;
  };

  return (
    <div className="space-y-4 h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold">Adapter Registry</h2>
          <p className="text-sm text-muted-foreground">Browse and manage trained adapters</p>
        </div>
        <Button>
          <Download className="h-4 w-4 mr-2" />
          Upload Adapter
        </Button>
      </div>

      {/* Search */}
      <div className="relative">
        <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="Search adapters by name or base model..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="pl-9"
        />
      </div>

      {/* Registry Table */}
      <Card className="flex-1">
        <CardContent className="p-0">
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              <span className="ml-3 text-muted-foreground">Loading adapters...</span>
            </div>
          ) : isError ? (
            <div className="flex flex-col items-center justify-center py-12">
              <AlertCircle className="h-8 w-8 text-destructive mb-2" />
              <p className="text-destructive font-medium">Failed to load adapters</p>
              <p className="text-sm text-muted-foreground mt-1">
                {error instanceof Error ? error.message : 'Unknown error'}
              </p>
            </div>
          ) : filteredAdapters.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12">
              <Search className="h-8 w-8 text-muted-foreground mb-2" />
              <p className="text-muted-foreground font-medium">No adapters found</p>
              <p className="text-sm text-muted-foreground mt-1">
                {searchTerm ? 'Try adjusting your search' : 'Upload an adapter to get started'}
              </p>
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Version</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Base Model</TableHead>
                  <TableHead>Performance</TableHead>
                  <TableHead>Size</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredAdapters.map((adapter) => (
                  <TableRow
                    key={adapter.id}
                    className={selectedAdapter === adapter.id ? 'bg-muted/50' : ''}
                    onClick={() => setSelectedAdapter(selectedAdapter === adapter.id ? null : adapter.id)}
                  >
                    <TableCell className="font-medium">{adapter.name}</TableCell>
                    <TableCell>{adapter.version}</TableCell>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        {getStatusIcon(adapter.status)}
                        {getStatusBadge(adapter.status)}
                      </div>
                    </TableCell>
                    <TableCell>{adapter.baseModel}</TableCell>
                    <TableCell>
                      {adapter.status === 'training' ? (
                        <span className="text-muted-foreground">Training...</span>
                      ) : (
                        `${adapter.performance}%`
                      )}
                    </TableCell>
                    <TableCell>{adapter.size}</TableCell>
                    <TableCell>
                      <div className="flex space-x-1">
                        <Button variant="ghost" size="sm">
                          <Eye className="h-4 w-4" />
                        </Button>
                        <Button variant="ghost" size="sm">
                          <Download className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Selected Adapter Details */}
      {selectedAdapter && !isLoading && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Adapter Details</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <strong>ID:</strong> {selectedAdapter}
              </div>
              <div>
                <strong>Created:</strong> {filteredAdapters.find(a => a.id === selectedAdapter)?.createdAt}
              </div>
              <div>
                <strong>Architecture:</strong> {(() => {
                  const adapter = data?.adapters.find(a => a.adapter_id === selectedAdapter);
                  return adapter?.rank ? `LoRA (r=${adapter.rank})` : 'LoRA';
                })()}
              </div>
              <div>
                <strong>Activation Count:</strong> {(() => {
                  const adapter = data?.adapters.find(a => a.adapter_id === selectedAdapter);
                  return adapter?.activation_count || 0;
                })()}
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
