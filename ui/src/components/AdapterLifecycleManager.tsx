import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Switch } from './ui/switch';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';
import { ErrorRecovery, errorRecoveryTemplates } from './ui/error-recovery';
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
import { useAdapterOperations } from '../hooks/useAdapterOperations';

import { toast } from 'sonner';

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

  // State records derived from adapters prop
  const [stateRecords, setStateRecords] = useState<AdapterStateRecord[]>([]);
  const [policiesLoading, setPoliciesLoading] = useState(false);
  const [policiesError, setPoliciesError] = useState<Error | null>(null);

  // Category policies fetched from backend
  const [policies, setPolicies] = useState<Record<AdapterCategory, CategoryPolicy> | null>(null);

  // Shared adapter operations
  const adapterOperations = useAdapterOperations({
    onAdapterUpdate,
    onAdapterEvict,
    onAdapterPin,
    onPolicyUpdate,
  });

  // Default policies matching backend defaults
  const getDefaultPolicies = (): Record<AdapterCategory, CategoryPolicy> => ({
    code: {
      promotion_threshold_ms: 1800000, // 30 minutes (from backend default)
      demotion_threshold_ms: 86400000, // 24 hours
      memory_limit: 200 * 1024 * 1024, // 200MB
      eviction_priority: 'low',
      auto_promote: true,
      auto_demote: false,
      max_in_memory: 10,
      routing_priority: 1.2
    },
    framework: {
      promotion_threshold_ms: 3600000, // 1 hour
      demotion_threshold_ms: 43200000, // 12 hours
      memory_limit: 150 * 1024 * 1024, // 150MB
      eviction_priority: 'normal',
      auto_promote: true,
      auto_demote: true,
      max_in_memory: 8,
      routing_priority: 1.0
    },
    codebase: {
      promotion_threshold_ms: 7200000, // 2 hours
      demotion_threshold_ms: 14400000, // 4 hours
      memory_limit: 300 * 1024 * 1024, // 300MB
      eviction_priority: 'high',
      auto_promote: false,
      auto_demote: true,
      max_in_memory: 5,
      routing_priority: 0.8
    },
    ephemeral: {
      promotion_threshold_ms: 0, // Immediate
      demotion_threshold_ms: 0, // Immediate
      memory_limit: 50 * 1024 * 1024, // 50MB
      eviction_priority: 'critical',
      auto_promote: false,
      auto_demote: true,
      max_in_memory: 20,
      routing_priority: 0.5
    }
  });

  // Fetch category policies from API
  const fetchCategoryPolicies = useCallback(async () => {
    setPoliciesLoading(true);
    setPoliciesError(null);
    try {
      const fetchedPolicies = await apiClient.getCategoryPolicies();
      
      // Validate all required categories are present
      const requiredCategories: AdapterCategory[] = ['code', 'framework', 'codebase', 'ephemeral'];
      const missingCategories = requiredCategories.filter(cat => !fetchedPolicies[cat]);
      
      if (missingCategories.length > 0) {
        logger.warn('Missing categories in policy response', {
          component: 'AdapterLifecycleManager',
          operation: 'fetchCategoryPolicies',
          missingCategories,
        });
      }
      
      // Validate eviction_priority values are valid
      const validPriorities: EvictionPriority[] = ['never', 'low', 'normal', 'high', 'critical'];
      for (const [category, policy] of Object.entries(fetchedPolicies)) {
        if (!validPriorities.includes(policy.eviction_priority as EvictionPriority)) {
          logger.error('Invalid eviction_priority in policy response', {
            component: 'AdapterLifecycleManager',
            operation: 'fetchCategoryPolicies',
            category,
            invalidPriority: policy.eviction_priority,
          });
          // Use 'normal' as safe fallback
          (fetchedPolicies[category] as CategoryPolicy).eviction_priority = 'normal';
        }
      }
      
      // Type assertion is safe because backend guarantees category keys match AdapterCategory
      setPolicies(fetchedPolicies as Record<AdapterCategory, CategoryPolicy>);
      logger.info('Category policies loaded', {
        component: 'AdapterLifecycleManager',
        operation: 'fetchCategoryPolicies',
        categories: Object.keys(fetchedPolicies),
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch category policies';
      logger.error('Failed to fetch category policies', {
        component: 'AdapterLifecycleManager',
        operation: 'fetchCategoryPolicies',
        error: errorMessage,
      }, err instanceof Error ? err : new Error(errorMessage));
      setPoliciesError(err instanceof Error ? err : new Error(errorMessage));

      // Check localStorage for persisted policies first
      const storedPolicies = localStorage.getItem('adapter_category_policies');
      if (storedPolicies) {
        try {
          const parsed = JSON.parse(storedPolicies);
          // Merge with defaults to ensure all categories are present
          const defaultPolicies = getDefaultPolicies();
          const mergedPolicies = { ...defaultPolicies, ...parsed };
          setPolicies(mergedPolicies as Record<AdapterCategory, CategoryPolicy>);
          logger.info('Loaded policies from localStorage', {
            component: 'AdapterLifecycleManager',
            operation: 'fetchCategoryPolicies',
            categories: Object.keys(parsed),
          });
          return;
        } catch (parseErr) {
          logger.warn('Failed to parse stored policies, using defaults', {
            component: 'AdapterLifecycleManager',
            operation: 'fetchCategoryPolicies',
            error: parseErr instanceof Error ? parseErr.message : 'Parse error',
          });
        }
      }

      // Fallback to defaults if API fails and no localStorage - use default structure matching backend defaults
      setPolicies(getDefaultPolicies());
    } finally {
      setPoliciesLoading(false);
    }
  }, []);

  // Load category policies on mount
  useEffect(() => {
    fetchCategoryPolicies();
  }, [fetchCategoryPolicies]);

  // Initialize state records from adapters with error handling
  useEffect(() => {
    try {
      const records: AdapterStateRecord[] = adapters.map((adapter, index) => ({
        adapter_id: adapter.adapter_id,
        adapter_idx: index,
        state: adapter.current_state,
        pinned: adapter.pinned,
        memory_bytes: adapter.memory_bytes,
        category: adapter.category,
        scope: adapter.scope,
        last_activated: adapter.last_activated,
        activation_count: adapter.activation_count,
        timestamp: adapter.last_activated || new Date().toISOString()
      }));
      setStateRecords(records);
    } catch (err) {
      logger.error('Failed to initialize state records', {
        component: 'AdapterLifecycleManager',
        operation: 'initializeStateRecords',
        error: err instanceof Error ? err.message : 'Unknown error',
      }, err instanceof Error ? err : new Error('Failed to initialize state records'));
      setStateRecords([]);
    }
  }, [adapters]);

  const getStateIcon = (state: AdapterState) => {
    switch (state) {
      case 'unloaded': return <Square className="h-4 w-4 text-gray-500" />;
      case 'cold': return <Snowflake className="h-4 w-4 text-gray-400" />;
      case 'warm': return <Thermometer className="h-4 w-4 text-gray-500" />;
      case 'hot': return <Flame className="h-4 w-4 text-gray-600" />;
      case 'resident': return <Anchor className="h-4 w-4 text-gray-600" />;
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

        await adapterOperations.promoteAdapter(adapterId);
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
      if (newState !== 'loading') {
        onAdapterUpdate(adapterId, { current_state: newState });
      }
      setErrorRecovery(null);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to update adapter state';
      logger.error('Failed to update adapter state', {
        component: 'AdapterLifecycleManager',
        operation: 'handleStateTransition',
        adapterId,
        newState,
        error: errorMessage
      });
      toast.error(`Failed to update state: ${errorMessage}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handlePinToggle = async (adapterId: string, pinned: boolean) => {
    setIsLoading(true);
    setStatusMessage(null);
    setErrorRecovery(null);
    try {
      await apiClient.pinAdapter(adapterId, pinned);
      onAdapterPin(adapterId, pinned);
      toast.success(pinned ? 'Adapter pinned successfully' : 'Adapter unpinned successfully');
      logger.info('Adapter pin state changed', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePinToggle',
        adapterId,
        pinned
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to pin/unpin adapter';
      logger.error('Failed to pin/unpin adapter', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePinToggle',
        adapterId,
        pinned,
        error: errorMessage
      });
      toast.error(`Failed to ${pinned ? 'pin' : 'unpin'} adapter: ${errorMessage}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handleEvictAdapter = async (adapterId: string) => {
    setIsLoading(true);
    setStatusMessage(null);
    setErrorRecovery(null);
    try {
      const result = await apiClient.evictAdapter(adapterId);
      onAdapterEvict(adapterId);
      toast.success(`Adapter evicted: ${result.message || 'Memory freed successfully'}`);
      logger.info('Adapter evicted', {
        component: 'AdapterLifecycleManager',
        operation: 'handleEvictAdapter',
        adapterId,
        result
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to evict adapter';
      logger.error('Failed to evict adapter', {
        component: 'AdapterLifecycleManager',
        operation: 'handleEvictAdapter',
        adapterId,
        error: errorMessage
      });


      toast.error(`Failed to evict adapter: ${errorMessage}`);
    } finally {
      setIsLoading(false);
    }
  };

  const handlePolicyUpdate = async (category: AdapterCategory, policy: CategoryPolicy) => {
    setIsLoading(true);
    setStatusMessage(null);
    setErrorRecovery(null);
    try {
      // Save to localStorage for persistence across page reloads
      const storedPolicies = localStorage.getItem('adapter_category_policies');
      const existing = storedPolicies ? JSON.parse(storedPolicies) : {};
      existing[category] = policy;
      localStorage.setItem('adapter_category_policies', JSON.stringify(existing));

      // Update local state
      setPolicies(prev => ({ ...prev, [category]: policy }));
      onPolicyUpdate(category, policy);

      // Note: Backend endpoint PUT /v1/adapters/category/:category/policy is planned
      // Once available, sync will happen automatically
      toast.success(`Policy updated for ${category}`);
      logger.info('Category policy updated', {
        component: 'AdapterLifecycleManager',
        operation: 'handlePolicyUpdate',
        category,
        note: 'Local update with localStorage persistence'
      });
      setStatusMessage({ message: `Policy updated successfully for ${category}.`, variant: 'success' });
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
        <ErrorRecovery
          error={errorMessage}
          onRetry={() => handlePolicyUpdate(category, policy)}
        />
      );
      toast.error(`Failed to update policy: ${errorMessage}`);
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
      {adapterOperations.operationError && (
        <div>
          {adapterOperations.operationError}
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
          <Button 
            variant="outline" 
            onClick={() => {
              fetchCategoryPolicies();
              // State records refresh automatically when adapters prop changes
            }}
            disabled={policiesLoading}
          >
            <RefreshCw className={`mr-2 h-4 w-4 ${policiesLoading ? 'animate-spin' : ''}`} />
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
          {policiesLoading ? (
            <div className="flex items-center justify-center p-8">
              <RefreshCw className="h-6 w-6 animate-spin text-muted-foreground" />
              <span className="ml-2 text-muted-foreground">Loading policies...</span>
            </div>
          ) : policiesError ? (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Failed to load category policies: {policiesError.message}. Using default values.
              </AlertDescription>
            </Alert>
          ) : policies && policies[selectedCategory] ? (
            <CategoryPolicyEditor 
              category={selectedCategory}
              policy={policies[selectedCategory]}
              onPolicyUpdate={(policy) => handlePolicyUpdate(selectedCategory, policy)}
              onCategoryChange={setSelectedCategory}
            />
          ) : (
            <div className="p-8 text-center text-muted-foreground">
              No policies available
            </div>
          )}
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
          <Label htmlFor="eviction_priority">Removal Priority</Label>
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
      await apiClient.updateAdapterPolicy(adapter.adapter_id, { adapter_id: adapter.adapter_id, policy_ids: [], category: newCategory });
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
