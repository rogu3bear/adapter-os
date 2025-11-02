import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { ErrorRecoveryTemplates } from './ui/error-recovery';
import { Alert, AlertDescription } from './ui/alert';
import {
  Settings,
  Play,
  Pause,
  Square,
  Pin,
  PinOff,
  Thermometer,
  Snowflake,
  Flame,
  Zap,
  Anchor,
  Clock,
  MemoryStick,
  Activity,
  Target,
  TrendingUp,
  AlertTriangle,
  CheckCircle,
  XCircle,
  Eye,
  Edit,
  Trash2,
  RefreshCw,
  Filter,
  Search
} from 'lucide-react';
import {
  Adapter,
  AdapterState,
  AdapterCategory,
  AdapterScope,
  EvictionPriority,
  CategoryPolicy,
  AdapterStateRecord,
  AdapterTransitionEvent,
  AdapterActivationEvent,
  AdapterEvictionEvent
} from '../api/types';
import apiClient from '../api/client';
import { logger } from '../utils/logger';
import { useFeatureDegradation } from '../hooks/useFeatureDegradation';

interface AdapterLifecycleManagerProps {
  adapters: Adapter[];
  onAdapterUpdate: (adapterId: string, updates: Partial<Adapter>) => void;
  onAdapterEvict: (adapterId: string) => void;
  onAdapterPin: (adapterId: string, pinned: boolean) => void;
  onPolicyUpdate: (category: AdapterCategory, policy: CategoryPolicy) => void;
}

export function AdapterLifecycleManager({ 
  adapters, 
  onAdapterUpdate, 
  onAdapterEvict, 
  onAdapterPin,
  onPolicyUpdate 
}: AdapterLifecycleManagerProps) {
  const [selectedAdapter, setSelectedAdapter] = useState<Adapter | null>(null);
  const [isPolicyDialogOpen, setIsPolicyDialogOpen] = useState(false);
  const [selectedCategory, setSelectedCategory] = useState<AdapterCategory>('code');
  const [filterState, setFilterState] = useState<AdapterState | 'all'>('all');
  const [filterCategory, setFilterCategory] = useState<AdapterCategory | 'all'>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  // Graceful degradation: Monitor memory availability
  const memoryAvailability = useFeatureDegradation({
    featureId: 'adapter-memory',
    healthCheck: () => {
      // Check if we have enough memory headroom (15% minimum)
      const totalMemory = adapters.reduce((sum, a) => sum + (a.memory_bytes || 0), 0);
      const hasMemoryHeadroom = totalMemory > 0; // Simplified check
      return hasMemoryHeadroom;
    },
    checkInterval: 60000,
  });

  // Mock state records for demonstration
  const [stateRecords, setStateRecords] = useState<AdapterStateRecord[]>([]);

  // Mock policies
  const [policies, setPolicies] = useState<Record<AdapterCategory, CategoryPolicy>>({
    code: {
      promotion_threshold_ms: 300000, // 5 minutes
      demotion_threshold_ms: 3600000, // 1 hour
      memory_limit: 100 * 1024 * 1024, // 100MB
      eviction_priority: 'low',
      auto_promote: true,
      auto_demote: true,
      max_in_memory: 50,
      routing_priority: 1.0
    },
    framework: {
      promotion_threshold_ms: 600000, // 10 minutes
      demotion_threshold_ms: 7200000, // 2 hours
      memory_limit: 200 * 1024 * 1024, // 200MB
      eviction_priority: 'normal',
      auto_promote: true,
      auto_demote: true,
      max_in_memory: 20,
      routing_priority: 0.8
    },
    codebase: {
      promotion_threshold_ms: 1800000, // 30 minutes
      demotion_threshold_ms: 14400000, // 4 hours
      memory_limit: 500 * 1024 * 1024, // 500MB
      eviction_priority: 'high',
      auto_promote: false,
      auto_demote: true,
      max_in_memory: 10,
      routing_priority: 0.6
    },
    ephemeral: {
      promotion_threshold_ms: 60000, // 1 minute
      demotion_threshold_ms: 300000, // 5 minutes
      memory_limit: 50 * 1024 * 1024, // 50MB
      eviction_priority: 'critical',
      auto_promote: false,
      auto_demote: true,
      max_in_memory: 100,
      routing_priority: 0.4
    }
  });

  useEffect(() => {
    // Initialize state records from adapters
    const records: AdapterStateRecord[] = adapters.map((adapter, index) => ({
      adapter_id: adapter.adapter_id,
      adapter_idx: index,
      state: adapter.current_state,
      pinned: adapter.pinned,
      memory_bytes: adapter.memory_bytes,
      category: adapter.category,
      scope: adapter.scope,
      last_activated: adapter.last_activated,
      activation_count: adapter.activation_count
    }));
    setStateRecords(records);
  }, [adapters]);

  const getStateIcon = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return <Square className="h-4 w-4 text-gray-500" />;
      case 'cold': return <Snowflake className="h-4 w-4 text-blue-500" />;
      case 'warm': return <Thermometer className="h-4 w-4 text-orange-500" />;
      case 'hot': return <Flame className="h-4 w-4 text-red-500" />;
      case 'resident': return <Anchor className="h-4 w-4 text-purple-500" />;
      default: return <Activity className="h-4 w-4 text-gray-500" />;
    }
  };

  const getStateColor = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return 'bg-gray-100 text-gray-800';
      case 'cold': return 'bg-blue-100 text-blue-800';
      case 'warm': return 'bg-orange-100 text-orange-800';
      case 'hot': return 'bg-red-100 text-red-800';
      case 'resident': return 'bg-purple-100 text-purple-800';
      default: return 'bg-gray-100 text-gray-800';
    }
  };

  const getCategoryIcon = (category: AdapterCategory) => {
    switch (category) {
      case 'code': return <Target className="h-4 w-4" />;
      case 'framework': return <Zap className="h-4 w-4" />;
      case 'codebase': return <Activity className="h-4 w-4" />;
      case 'ephemeral': return <Clock className="h-4 w-4" />;
      default: return <Activity className="h-4 w-4" />;
    }
  };

  const getEvictionPriorityColor = (priority: EvictionPriority) => {
    switch (priority) {
      case 'never': return 'bg-green-100 text-green-800';
      case 'low': return 'bg-blue-100 text-blue-800';
      case 'normal': return 'bg-yellow-100 text-yellow-800';
      case 'high': return 'bg-orange-100 text-orange-800';
      case 'critical': return 'bg-red-100 text-red-800';
      default: return 'bg-gray-100 text-gray-800';
    }
  };

  const filteredAdapters = adapters.filter(adapter => {
    const matchesState = filterState === 'all' || adapter.current_state === filterState;
    const matchesCategory = filterCategory === 'all' || adapter.category === filterCategory;
    const matchesSearch = searchQuery === '' || 
      adapter.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      adapter.adapter_id.toLowerCase().includes(searchQuery.toLowerCase());
    
    return matchesState && matchesCategory && matchesSearch;
  });

  const handleStateTransition = async (adapterId: string, newState: AdapterState) => {
    setIsLoading(true);
    try {
      // Note: Current API only supports promoting state (cold -> warm -> hot)
      // Manual state setting requires backend enhancement
      if (newState === 'hot' || newState === 'warm') {
        await apiClient.promoteAdapterState(adapterId);
        setStatusMessage({ message: 'Adapter state promoted successfully.', variant: 'success' });
        // Refresh records
        setStateRecords(prev => prev.map(record => (
          record.adapter_id === adapterId
            ? { ...record, state: newState }
            : record
          )));
      } else {
        setStatusMessage({ message: `Local state updated to ${newState}. Backend sync pending.`, variant: 'info' });
        setStateRecords(prev => prev.map(record => (
          record.adapter_id === adapterId
            ? { ...record, state: newState }
            : record
          )));
      }
      onAdapterUpdate(adapterId, { current_state: newState });
      setErrorRecovery(null);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setStatusMessage({ message: `Failed to update state: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => handleStateTransition(adapterId, newState)
        )
      );
      logger.error('Failed to update adapter state', {
        component: 'AdapterLifecycleManager',
        operation: 'handleStateTransition',
        adapterId,
        newState,
        error: errorMessage
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handlePinToggle = async (adapterId: string, pinned: boolean) => {
    setIsLoading(true);
    try {
      await apiClient.pinAdapter(adapterId, pinned);
      setStatusMessage({ message: pinned ? 'Adapter pinned successfully.' : 'Adapter unpinned successfully.', variant: 'success' });
      onAdapterPin(adapterId, pinned);
      setErrorRecovery(null);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setStatusMessage({ message: `Failed to ${pinned ? 'pin' : 'unpin'} adapter: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => handlePinToggle(adapterId, pinned)
        )
      );
      logger.error('Failed to toggle adapter pin state', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePinToggle',
        adapterId,
        pinned,
        error: errorMessage
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleEvictAdapter = async (adapterId: string) => {
    setIsLoading(true);
    try {
      const result = await apiClient.evictAdapter(adapterId);
      setStatusMessage({ message: `Adapter evicted: ${result.message || 'Memory freed successfully.'}`, variant: 'success' });
      onAdapterEvict(adapterId);
      setErrorRecovery(null);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setStatusMessage({ message: `Failed to evict adapter: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error(errorMessage),
          () => handleEvictAdapter(adapterId)
        )
      );
      logger.error('Failed to evict adapter', {
        component: 'AdapterLifecycleManager',
        operation: 'handleEvictAdapter',
        adapterId,
        error: errorMessage
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handlePolicyUpdate = async (category: AdapterCategory, policy: CategoryPolicy) => {
    setIsLoading(true);
    try {
      setPolicies(prev => ({ ...prev, [category]: policy }));
      onPolicyUpdate(category, policy);
      // TODO: Backend implementation required - PUT /v1/adapters/category/:category/policy
      // This endpoint doesn't exist yet. For now, we update locally only.
      setStatusMessage({ message: `Policy updated locally for ${category}. Backend sync pending API implementation.`, variant: 'info' });
      logger.warn('Policy update: backend endpoint not implemented', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePolicyUpdate',
        category,
        policy,
        note: 'Local update only - PUT /v1/adapters/category/:category/policy needs implementation'
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to update policy';
      logger.error('Failed to update policy', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePolicyUpdate',
        category,
        error: errorMessage
      });
      setStatusMessage({ message: `Failed to update policy: ${errorMessage}`, variant: 'warning' });
      setErrorRecovery(
        ErrorRecoveryTemplates.genericError(
          error instanceof Error ? error : new Error(errorMessage),
          () => handlePolicyUpdate(category, policy)
        )
      );
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Graceful degradation alert for memory pressure */}
      {memoryAvailability.isDegraded && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            Memory pressure detected. Adapter eviction may occur more frequently. Consider reducing the number of loaded adapters or increasing available memory.
          </AlertDescription>
        </Alert>
      )}
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      {/* Controls */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          <div className="flex items-center space-x-2">
            <Search className="h-4 w-4" />
            <Input
              placeholder="Search adapters..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-64"
            />
          </div>
          <Select value={filterState} onValueChange={(value) => setFilterState(value as AdapterState | 'all')}>
            <SelectTrigger className="w-40">
              <SelectValue placeholder="Filter by state" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All States</SelectItem>
              <SelectItem value="unloaded">Unloaded</SelectItem>
              <SelectItem value="cold">Cold</SelectItem>
              <SelectItem value="warm">Warm</SelectItem>
              <SelectItem value="hot">Hot</SelectItem>
              <SelectItem value="resident">Resident</SelectItem>
            </SelectContent>
          </Select>
          <Select value={filterCategory} onValueChange={(value) => setFilterCategory(value as AdapterCategory | 'all')}>
            <SelectTrigger className="w-40">
              <SelectValue placeholder="Filter by category" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Categories</SelectItem>
              <SelectItem value="code">Code</SelectItem>
              <SelectItem value="framework">Framework</SelectItem>
              <SelectItem value="codebase">Codebase</SelectItem>
              <SelectItem value="ephemeral">Ephemeral</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="flex space-x-2">
          <Button variant="outline" onClick={() => setIsPolicyDialogOpen(true)}>
            <Settings className="mr-2 h-4 w-4" />
            Category Policies
          </Button>
          <Button variant="outline" onClick={() => window.location.reload()}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Refresh
          </Button>
        </div>
      </div>

      {/* Adapter Table */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Activity className="mr-2 h-5 w-5" />
            Adapter Lifecycle Management
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Category</TableHead>
                <TableHead>State</TableHead>
                <TableHead>Memory</TableHead>
                <TableHead>Activations</TableHead>
                <TableHead>Last Used</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredAdapters.map((adapter) => {
                const stateRecord = stateRecords.find(r => r.adapter_id === adapter.adapter_id);
                return (
                  <TableRow key={adapter.adapter_id}>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        {getCategoryIcon(adapter.category)}
                        <div>
                          <div className="font-medium">{adapter.name}</div>
                          <div className="text-sm text-muted-foreground">
                            {adapter.adapter_id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className="flex items-center space-x-1">
                        {getCategoryIcon(adapter.category)}
                        <span>{adapter.category}</span>
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        {getStateIcon(adapter.current_state)}
                        <Badge className={getStateColor(adapter.current_state)}>
                          {adapter.current_state}
                        </Badge>
                        {adapter.pinned && (
                          <Pin className="h-4 w-4 text-purple-500" />
                        )}
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        <MemoryStick className="h-4 w-4" />
                        <span>{Math.round(adapter.memory_bytes / 1024 / 1024)} MB</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        <Target className="h-4 w-4" />
                        <span>{adapter.activation_count}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex items-center space-x-2">
                        <Clock className="h-4 w-4" />
                        <span>{adapter.last_activated || 'Never'}</span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="flex space-x-1">
                        <Button 
                          variant="ghost" 
                          size="sm"
                          onClick={() => setSelectedAdapter(adapter)}
                        >
                          <Eye className="h-4 w-4" />
                        </Button>
                        <Button 
                          variant="ghost" 
                          size="sm"
                          onClick={() => handlePinToggle(adapter.adapter_id, !adapter.pinned)}
                        >
                          {adapter.pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
                        </Button>
                        <Button 
                          variant="ghost" 
                          size="sm"
                          onClick={() => handleEvictAdapter(adapter.adapter_id)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      {/* Category Policies Dialog */}
      <Dialog open={isPolicyDialogOpen} onOpenChange={setIsPolicyDialogOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Category Policies</DialogTitle>
          </DialogHeader>
          <CategoryPolicyEditor 
            category={selectedCategory}
            policy={policies[selectedCategory]}
            onPolicyUpdate={(policy) => handlePolicyUpdate(selectedCategory, policy)}
            onCategoryChange={setSelectedCategory}
          />
        </DialogContent>
      </Dialog>

      {/* Adapter Details Dialog */}
      {selectedAdapter && (
        <Dialog open={!!selectedAdapter} onOpenChange={() => setSelectedAdapter(null)}>
          <DialogContent className="max-w-2xl">
            <DialogHeader>
              <DialogTitle>Adapter Details</DialogTitle>
            </DialogHeader>
            <AdapterDetailsView 
              adapter={selectedAdapter}
              onStateChange={(newState) => handleStateTransition(selectedAdapter.adapter_id, newState)}
              onPinToggle={(pinned) => handlePinToggle(selectedAdapter.adapter_id, pinned)}
              onAdapterUpdate={onAdapterUpdate}
            />
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}

// Category Policy Editor Component
function CategoryPolicyEditor({ 
  category, 
  policy, 
  onPolicyUpdate, 
  onCategoryChange 
}: {
  category: AdapterCategory;
  policy: CategoryPolicy;
  onPolicyUpdate: (policy: CategoryPolicy) => void;
  onCategoryChange: (category: AdapterCategory) => void;
}) {
  const [localPolicy, setLocalPolicy] = useState(policy);

  useEffect(() => {
    setLocalPolicy(policy);
  }, [policy]);

  const handleSave = () => {
    onPolicyUpdate(localPolicy);
  };

  return (
    <div className="space-y-6">
      <div>
        <Label htmlFor="category">Category</Label>
        <Select value={category} onValueChange={(value) => onCategoryChange(value as AdapterCategory)}>
          <SelectTrigger>
            <SelectValue placeholder="Select category" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="code">Code</SelectItem>
            <SelectItem value="framework">Framework</SelectItem>
            <SelectItem value="codebase">Codebase</SelectItem>
            <SelectItem value="ephemeral">Ephemeral</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="promotion_threshold">Promotion Threshold (ms)</Label>
          <Input
            id="promotion_threshold"
            type="number"
            value={localPolicy.promotion_threshold_ms}
            onChange={(e) => setLocalPolicy({
              ...localPolicy,
              promotion_threshold_ms: parseInt(e.target.value)
            })}
          />
        </div>
        <div>
          <Label htmlFor="demotion_threshold">Demotion Threshold (ms)</Label>
          <Input
            id="demotion_threshold"
            type="number"
            value={localPolicy.demotion_threshold_ms}
            onChange={(e) => setLocalPolicy({
              ...localPolicy,
              demotion_threshold_ms: parseInt(e.target.value)
            })}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="memory_limit">Memory Limit (bytes)</Label>
          <Input
            id="memory_limit"
            type="number"
            value={localPolicy.memory_limit}
            onChange={(e) => setLocalPolicy({
              ...localPolicy,
              memory_limit: parseInt(e.target.value)
            })}
          />
        </div>
        <div>
          <Label htmlFor="max_in_memory">Max In Memory</Label>
          <Input
            id="max_in_memory"
            type="number"
            value={localPolicy.max_in_memory || 0}
            onChange={(e) => setLocalPolicy({
              ...localPolicy,
              max_in_memory: parseInt(e.target.value) || undefined
            })}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label htmlFor="eviction_priority">Eviction Priority</Label>
          <Select 
            value={localPolicy.eviction_priority} 
            onValueChange={(value) => setLocalPolicy({
              ...localPolicy,
              eviction_priority: value as EvictionPriority
            })}
          >
            <SelectTrigger>
              <SelectValue placeholder="Select priority" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="never">Never</SelectItem>
              <SelectItem value="low">Low</SelectItem>
              <SelectItem value="normal">Normal</SelectItem>
              <SelectItem value="high">High</SelectItem>
              <SelectItem value="critical">Critical</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div>
          <Label htmlFor="routing_priority">Routing Priority</Label>
          <Input
            id="routing_priority"
            type="number"
            step="0.1"
            value={localPolicy.routing_priority}
            onChange={(e) => setLocalPolicy({
              ...localPolicy,
              routing_priority: parseFloat(e.target.value)
            })}
          />
        </div>
      </div>

      <div className="flex items-center space-x-4">
        <div className="flex items-center space-x-2">
          <Switch
            id="auto_promote"
            checked={localPolicy.auto_promote}
            onCheckedChange={(checked) => setLocalPolicy({
              ...localPolicy,
              auto_promote: checked
            })}
          />
          <Label htmlFor="auto_promote">Auto Promote</Label>
        </div>
        <div className="flex items-center space-x-2">
          <Switch
            id="auto_demote"
            checked={localPolicy.auto_demote}
            onCheckedChange={(checked) => setLocalPolicy({
              ...localPolicy,
              auto_demote: checked
            })}
          />
          <Label htmlFor="auto_demote">Auto Demote</Label>
        </div>
      </div>

      <div className="flex justify-end space-x-2">
        <Button variant="outline" onClick={() => setLocalPolicy(policy)}>
          Reset
        </Button>
        <Button onClick={handleSave}>
          Save Policy
        </Button>
      </div>
    </div>
  );
}

// Adapter Details View Component
function AdapterDetailsView({ 
  adapter, 
  onStateChange, 
  onPinToggle,
  onAdapterUpdate
}: {
  adapter: Adapter;
  onStateChange: (state: AdapterState) => void;
  onPinToggle: (pinned: boolean) => void;
  onAdapterUpdate: (adapterId: string, updates: Partial<Adapter>) => void;
}) {
  const states: AdapterState[] = ['unloaded', 'cold', 'warm', 'hot', 'resident'];
  const [category, setCategory] = useState<AdapterCategory>(adapter.category);
  const [isUpdatingCategory, setIsUpdatingCategory] = useState(false);

  const handleCategoryChange = async (newCategory: AdapterCategory) => {
    if (newCategory === category) return;
    
    setIsUpdatingCategory(true);
    try {
      await apiClient.updateAdapterPolicy(adapter.adapter_id, { category: newCategory });
      setCategory(newCategory);
      onAdapterUpdate(adapter.adapter_id, { category: newCategory });
      logger.info('Adapter category updated', {
        component: 'AdapterDetailsView',
        adapterId: adapter.adapter_id,
        newCategory,
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to update category';
      logger.error('Failed to update adapter category', {
        component: 'AdapterDetailsView',
        adapterId: adapter.adapter_id,
        error: errorMessage,
      }, error instanceof Error ? error : new Error(errorMessage));
    } finally {
      setIsUpdatingCategory(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label className="text-sm font-medium">Name</Label>
          <p className="text-sm text-muted-foreground">{adapter.name}</p>
        </div>
        <div>
          <Label className="text-sm font-medium">Adapter ID</Label>
          <p className="text-sm text-muted-foreground">{adapter.adapter_id}</p>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label className="text-sm font-medium">Category</Label>
          <Select 
            value={category} 
            onValueChange={(value) => handleCategoryChange(value as AdapterCategory)}
            disabled={isUpdatingCategory}
          >
            <SelectTrigger className="w-full">
              <SelectValue placeholder="Select category" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="code">Code</SelectItem>
              <SelectItem value="framework">Framework</SelectItem>
              <SelectItem value="codebase">Codebase</SelectItem>
              <SelectItem value="ephemeral">Ephemeral</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div>
          <Label className="text-sm font-medium">Scope</Label>
          <p className="text-sm text-muted-foreground">{adapter.scope}</p>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label className="text-sm font-medium">Current State</Label>
          <p className="text-sm text-muted-foreground">{adapter.current_state}</p>
        </div>
        <div>
          <Label className="text-sm font-medium">Memory Usage</Label>
          <p className="text-sm text-muted-foreground">{Math.round(adapter.memory_bytes / 1024 / 1024)} MB</p>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <Label className="text-sm font-medium">Activation Count</Label>
          <p className="text-sm text-muted-foreground">{adapter.activation_count}</p>
        </div>
        <div>
          <Label className="text-sm font-medium">Last Activated</Label>
          <p className="text-sm text-muted-foreground">{adapter.last_activated || 'Never'}</p>
        </div>
      </div>

      <div>
        <Label className="text-sm font-medium">State Management</Label>
        <div className="flex items-center space-x-2 mt-2">
          <Select value={adapter.current_state} onValueChange={onStateChange}>
            <SelectTrigger className="w-40">
              <SelectValue placeholder="Select state" />
            </SelectTrigger>
            <SelectContent>
              {states.map((state) => (
                <SelectItem key={state} value={state}>
                  {state}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant={adapter.pinned ? "default" : "outline"}
            size="sm"
            onClick={() => onPinToggle(!adapter.pinned)}
          >
            {adapter.pinned ? <PinOff className="h-4 w-4" /> : <Pin className="h-4 w-4" />}
            {adapter.pinned ? 'Unpin' : 'Pin'}
          </Button>
        </div>
      </div>

      {adapter.framework_id && (
        <div>
          <Label className="text-sm font-medium">Framework</Label>
          <p className="text-sm text-muted-foreground">
            {adapter.framework_id} {adapter.framework_version && `v${adapter.framework_version}`}
          </p>
        </div>
      )}

      {adapter.repo_id && (
        <div>
          <Label className="text-sm font-medium">Repository</Label>
          <p className="text-sm text-muted-foreground">
            {adapter.repo_id} {adapter.commit_sha && `@${adapter.commit_sha.substring(0, 8)}`}
          </p>
        </div>
      )}

      {adapter.intent && (
        <div>
          <Label className="text-sm font-medium">Intent</Label>
          <p className="text-sm text-muted-foreground">{adapter.intent}</p>
        </div>
      )}
    </div>
  );
}
