import React from 'react';
import { AdvancedFilter, type FilterConfig, type FilterValues } from '@/components/ui/advanced-filter';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Checkbox } from '@/components/ui/checkbox';
import { EmptyState } from '@/components/ui/empty-state';
import { VirtualizedTableRows } from '@/components/ui/virtualized-table';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { BookmarkButton } from '@/components/ui/bookmark-button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { Adapter } from '@/api/types';
import { ExportScope } from '@/components/ui/export-dialog';
import { Pin, Play, Pause, MoreHorizontal, ArrowUp, Download, Activity, Trash2, Code, Target, Clock } from 'lucide-react';
import { getCategoryIcon, getStateIcon } from '@/components/adapters/helpers';
import { getLifecycleVariant } from '@/utils/lifecycle';

interface AdapterRegistryTabProps {
  adapters: Adapter[];
  filteredAdapters: Adapter[];
  selectedAdapters: string[];
  setSelectedAdapters: React.Dispatch<React.SetStateAction<string[]>>;
  adapterFilterConfigs: FilterConfig[];
  filterValues: FilterValues;
  setFilterValues: React.Dispatch<React.SetStateAction<FilterValues>>;
  setExportDialogScope: React.Dispatch<React.SetStateAction<ExportScope>>;
  setShowExportDialog: React.Dispatch<React.SetStateAction<boolean>>;
  handleLoadAdapter: (adapterId: string) => void;
  handleUnloadAdapter: (adapterId: string) => void;
  handlePinToggle: (adapter: Adapter) => void;
  handlePromoteState: (adapterId: string) => void;
  handleViewHealth: (adapterId: string) => Promise<void>;
  handleDownloadManifest: (adapterId: string) => void;
  setDeleteConfirmId: React.Dispatch<React.SetStateAction<string | null>>;
}

export function AdapterRegistryTab({
  adapters,
  filteredAdapters,
  selectedAdapters,
  setSelectedAdapters,
  adapterFilterConfigs,
  filterValues,
  setFilterValues,
  setExportDialogScope,
  setShowExportDialog,
  handleLoadAdapter,
  handleUnloadAdapter,
  handlePinToggle,
  handlePromoteState,
  handleViewHealth,
  handleDownloadManifest,
  setDeleteConfirmId,
}: AdapterRegistryTabProps) {
  const allSelected = filteredAdapters.length > 0 && filteredAdapters.every(adapter => selectedAdapters.includes(adapter.adapter_id));
  const someSelected = selectedAdapters.length > 0 && filteredAdapters.some(adapter => selectedAdapters.includes(adapter.adapter_id));

  return (
    <>
      <AdvancedFilter
        configs={adapterFilterConfigs}
        values={filterValues}
        onChange={setFilterValues}
        className="mb-4"
        title="Filter Adapters"
      />
      <Card className="card-standard">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle className="flex items-center justify-center">
              Registered Adapters
              {filteredAdapters.length !== adapters.length && (
                <span className="ml-2 text-sm font-normal text-muted-foreground">
                  ({filteredAdapters.length} of {adapters.length})
                </span>
              )}
            </CardTitle>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setExportDialogScope(selectedAdapters.length > 0 ? 'selected' : 'all');
                setShowExportDialog(true);
              }}
            >
              <Download className="h-4 w-4 mr-2" />
              Export
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div className="max-h-[600px] overflow-auto" data-virtual-container>
            <Table className="border-collapse w-full" role="table" aria-label="Registered adapters">
              <TableHeader>
                <TableRow role="row">
                  <TableHead className="p-4 border-b border-border w-12" role="columnheader" scope="col">
                    <Checkbox
                      checked={filteredAdapters.length === 0 ? false : allSelected ? true : someSelected ? 'indeterminate' : false}
                      onCheckedChange={(checked) => {
                        if (checked) {
                          setSelectedAdapters(filteredAdapters.map(a => a.adapter_id));
                        } else {
                          setSelectedAdapters([]);
                        }
                      }}
                      aria-label="Select all adapters"
                    />
                  </TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Name</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Category</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Version</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Lifecycle</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">State</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Memory</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Activations</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Last Used</TableHead>
                  <TableHead className="p-4 border-b border-border" role="columnheader" scope="col">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredAdapters.length === 0 ? (
                  <TableRow role="row">
                    <TableCell colSpan={10} className="h-32" role="gridcell" aria-live="polite">
                      <EmptyState
                        icon={Code}
                        title={adapters.length === 0 ? "No Adapters Registered" : "No Adapters Match Filters"}
                        description={adapters.length === 0
                          ? "Get started by registering your first adapter or training a new one from your codebase. Use the Register or Train buttons above to begin."
                          : "Try adjusting your filters to see more results."}
                      />
                    </TableCell>
                  </TableRow>
                ) : (
                  <VirtualizedTableRows items={filteredAdapters} estimateSize={60}>
                    {(adapter) => {
                      const adapterTyped = adapter as Adapter;
                      const lifecycleVariant = getLifecycleVariant(adapterTyped.lifecycle_state);
                      return (
                        <TableRow key={adapterTyped.id} role="row">
                          <TableCell className="p-4 border-b border-border" role="gridcell">
                            <Checkbox
                              checked={selectedAdapters.includes(adapterTyped.adapter_id)}
                              onCheckedChange={(checked) => {
                                if (checked) {
                                  setSelectedAdapters(prev => [...prev, adapterTyped.adapter_id]);
                                } else {
                                  setSelectedAdapters(prev => prev.filter(id => id !== adapterTyped.adapter_id));
                                }
                              }}
                              aria-label={`Select ${adapterTyped.name}`}
                            />
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center justify-center gap-2">
                              {getCategoryIcon(adapterTyped.category)}
                              <div>
                                <div className="font-medium">{adapterTyped.name}</div>
                                <div className="text-sm text-muted-foreground">
                                  Tier {adapterTyped.tier} • Rank {adapterTyped.rank}
                                </div>
                              </div>
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border" role="gridcell">
                            <div className="status-indicator status-neutral flex items-center justify-center gap-2">
                              {getCategoryIcon(adapterTyped.category)}
                              <span>{adapterTyped.category}</span>
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border text-sm text-muted-foreground">
                            {adapterTyped.version || '1.0.0'}
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <Badge variant={lifecycleVariant}>
                              {adapterTyped.lifecycle_state || 'active'}
                            </Badge>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center justify-center gap-2">
                              {getStateIcon(adapterTyped.current_state)}
                              <span className="text-sm font-mono">{adapterTyped.current_state}</span>
                              {adapterTyped.pinned && (
                                <Pin className="h-4 w-4 text-gray-600" />
                              )}
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center justify-center gap-2">
                              <span className="text-sm">{Math.round(adapterTyped.memory_bytes / 1024 / 1024)} MB</span>
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center justify-center gap-2">
                              <Target className="h-4 w-4" />
                              <span>{adapterTyped.activation_count}</span>
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center justify-center gap-2">
                              <Clock className="h-4 w-4" />
                              <span>{adapterTyped.last_activated ? new Date(adapterTyped.last_activated).toLocaleString() : 'Never'}</span>
                            </div>
                          </TableCell>
                          <TableCell className="p-4 border-b border-border">
                            <div className="flex items-center gap-1">
                              <BookmarkButton
                                type="adapter"
                                title={adapterTyped.name}
                                url={`/adapters?adapter=${encodeURIComponent(adapterTyped.adapter_id)}`}
                                entityId={adapterTyped.adapter_id}
                                description={`${adapterTyped.framework || 'Unknown'} • ${adapterTyped.category || 'Unknown category'}`}
                                variant="ghost"
                                size="icon"
                              />
                              <DropdownMenu>
                                <DropdownMenuTrigger asChild>
                                  <Button variant="ghost" size="sm" aria-label={`Actions for ${adapterTyped.name}`}>
                                    <MoreHorizontal className="h-4 w-4" />
                                  </Button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end">
                                  {['warm', 'hot', 'resident'].includes(adapterTyped.current_state) ? (
                                    <DropdownMenuItem onClick={() => handleUnloadAdapter(adapterTyped.adapter_id)}>
                                      <Pause className="mr-2 h-4 w-4" />
                                      Unload
                                    </DropdownMenuItem>
                                  ) : (
                                    <DropdownMenuItem onClick={() => handleLoadAdapter(adapterTyped.adapter_id)}>
                                      <Play className="mr-2 h-4 w-4" />
                                      Load
                                    </DropdownMenuItem>
                                  )}
                                  <DropdownMenuItem onClick={() => handlePinToggle(adapterTyped)}>
                                    <Pin className="mr-2 h-4 w-4" />
                                    {adapterTyped.pinned ? 'Unpin' : 'Pin'}
                                  </DropdownMenuItem>
                                  <DropdownMenuItem onClick={() => handlePromoteState(adapterTyped.adapter_id)}>
                                    <ArrowUp className="mr-2 h-4 w-4" />
                                    Promote State
                                  </DropdownMenuItem>
                                  <DropdownMenuItem onClick={() => handleViewHealth(adapterTyped.adapter_id)}>
                                    <Activity className="mr-2 h-4 w-4" />
                                    View Health
                                  </DropdownMenuItem>
                                  <DropdownMenuItem onClick={() => handleDownloadManifest(adapterTyped.adapter_id)}>
                                    <Download className="mr-2 h-4 w-4" />
                                    Download Manifest
                                  </DropdownMenuItem>
                                  <DropdownMenuItem onClick={() => setDeleteConfirmId(adapterTyped.adapter_id)}>
                                    <Trash2 className="mr-2 h-4 w-4 text-gray-700" />
                                    Delete
                                  </DropdownMenuItem>
                                </DropdownMenuContent>
                              </DropdownMenu>
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    }}
                  </VirtualizedTableRows>
                )}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>
    </>
  );
}
