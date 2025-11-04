import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui/table';
import { Search, Download, Eye, Clock, CheckCircle, XCircle } from 'lucide-react';

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

const mockAdapters: AdapterEntry[] = [
  {
    id: 'adapter-001',
    name: 'Code Generation v2',
    version: '2.1.0',
    status: 'active',
    baseModel: 'Qwen2.5-7B',
    performance: 87.3,
    createdAt: '2024-01-15',
    size: '1.2GB'
  },
  {
    id: 'adapter-002',
    name: 'Medical Analysis',
    version: '1.0.3',
    status: 'active',
    baseModel: 'Qwen2.5-7B',
    performance: 92.1,
    createdAt: '2024-01-12',
    size: '956MB'
  },
  {
    id: 'adapter-003',
    name: 'Financial Analysis v1',
    version: '1.2.1',
    status: 'training',
    baseModel: 'Qwen2.5-7B',
    performance: 0,
    createdAt: '2024-01-14',
    size: 'In Progress'
  }
];

export default function MLEngineerRegistryBrowser() {
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedAdapter, setSelectedAdapter] = useState<string | null>(null);

  const filteredAdapters = mockAdapters.filter(adapter =>
    adapter.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    adapter.baseModel.toLowerCase().includes(searchTerm.toLowerCase())
  );

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
        </CardContent>
      </Card>

      {/* Selected Adapter Details */}
      {selectedAdapter && (
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
                <strong>Created:</strong> {mockAdapters.find(a => a.id === selectedAdapter)?.createdAt}
              </div>
              <div>
                <strong>Architecture:</strong> LoRA (r=8, α=16)
              </div>
              <div>
                <strong>Training Data:</strong> 1.2M tokens
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
