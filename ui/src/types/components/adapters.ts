/**
 * Component prop types for adapter-related UI components
 *
 * These types define the interfaces for components that manage and display adapters,
 * including lifecycle management, memory monitoring, stack composition, and state visualization.
 */

import type {
  Adapter,
  AdapterState,
  AdapterCategory,
  AdapterScope,
  EvictionPriority,
  CategoryPolicy,
  AdapterStateRecord,
  AdapterTransitionEvent,
  AdapterActivationEvent,
  AdapterEvictionEvent,
  MemoryUsageByCategory,
} from '@/api/types';

/**
 * Represents an adapter within a stack with ordering and enablement
 */
export interface StackAdapter {
  adapter: Adapter;
  order: number;
  enabled: boolean;
}

/**
 * Props for AdapterImportWizard component
 * Multi-step wizard for importing adapter files with validation
 */
export interface AdapterImportWizardProps {
  /** Callback invoked when import completes successfully */
  onComplete: (adapter: Adapter) => void;
  /** Callback invoked when user cancels the import */
  onCancel: () => void;
  /** Optional tenant ID to scope the imported adapter */
  tenantId?: string;
}

/**
 * Props for AdapterStackComposer component
 * Interface for composing and managing adapter stacks
 */
export interface AdapterStackComposerProps {
  /** Callback invoked when a new stack is created */
  onStackCreated?: (stackId: string, stackName: string) => void;
  /** Callback invoked when an existing stack is updated */
  onStackUpdated?: (stackId: string, adapters: StackAdapter[]) => void;
  /** Initial stack ID for editing an existing stack */
  initialStackId?: string;
  /** Initial stack name for editing an existing stack */
  initialStackName?: string;
  /** Initial adapters for editing an existing stack */
  initialAdapters?: StackAdapter[];
}

/**
 * Props for AdapterLifecycleManager component
 * Manages adapter lifecycle states, eviction, and category policies
 */
export interface AdapterLifecycleManagerProps {
  /** List of adapters to manage */
  adapters: Adapter[];
  /** Callback invoked when an adapter needs to be updated */
  onAdapterUpdate: (adapterId: string, updates: Partial<Adapter>) => void;
  /** Callback invoked when an adapter should be evicted */
  onAdapterEvict: (adapterId: string) => void;
  /** Callback invoked when an adapter's pinned status changes */
  onAdapterPin: (adapterId: string, pinned: boolean) => void;
  /** Callback invoked when a category policy should be updated */
  onPolicyUpdate: (category: AdapterCategory, policy: CategoryPolicy) => void;
}

/**
 * Props for AdapterMemoryMonitor component
 * Monitors and manages adapter memory usage with eviction controls
 */
export interface AdapterMemoryMonitorProps {
  /** List of adapters to monitor */
  adapters: Adapter[];
  /** Total available memory in bytes */
  totalMemory: number;
  /** Callback invoked when an adapter should be evicted */
  onEvictAdapter: (adapterId: string) => void;
  /** Callback invoked when an adapter's pinned status changes */
  onPinAdapter: (adapterId: string, pinned: boolean) => void;
  /** Callback invoked when a category's memory limit should be updated */
  onUpdateMemoryLimit: (category: AdapterCategory, limit: number) => void;
}

/**
 * Props for SortableAdapterItem component
 * Draggable adapter item for stack composition
 */
export interface SortableAdapterItemProps {
  /** Stack adapter item to display */
  item: StackAdapter;
  /** Callback invoked when the item should be removed */
  onRemove: () => void;
  /** Callback invoked when the item's enabled status should be toggled */
  onToggle: () => void;
}

/**
 * Props for AdapterStateVisualization component
 * Visualizes adapter states and memory distribution
 */
export interface AdapterStateVisualizationProps {
  /** List of adapter state records to visualize */
  adapters: AdapterStateRecord[];
  /** Total available memory in bytes */
  totalMemory: number;
}

/**
 * Domain type for domain-specific adapters
 */
export type DomainAdapterDomain = 'text' | 'vision' | 'telemetry';

/**
 * Domain adapter interface (client-side)
 */
export interface DomainAdapter {
  id: string;
  name: string;
  version?: string;
  description: string;
  domain_type: DomainAdapterDomain;
  model: string;
  config?: Record<string, unknown>;
  created_at?: string;
  updated_at?: string;
}

/**
 * Props for DomainAdapterManager component
 * Manages domain-specific adapters (text, vision, telemetry)
 */
export interface DomainAdapterManagerProps {
  /** Current user */
  user: {
    id: string;
    name?: string;
    email?: string;
    role?: string;
    [key: string]: unknown;
  };
  /** Selected tenant ID */
  selectedTenant: string;
}

/**
 * Props for adapter loading status display
 */
export interface AdapterLoadingStatusProps {
  /** Adapter ID being loaded */
  adapterId: string;
  /** Adapter name for display */
  adapterName?: string;
  /** Current loading state */
  state: 'unloaded' | 'cold' | 'warm' | 'hot' | 'resident';
  /** Loading progress percentage (0-100) */
  progress?: number;
  /** Whether the adapter is currently loading */
  isLoading?: boolean;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for adapter loading progress indicator
 */
export interface AdapterLoadingProgressProps {
  /** List of adapters being loaded */
  adapters: Array<{
    id: string;
    name: string;
    state: AdapterState;
    progress?: number;
  }>;
  /** Overall loading progress percentage (0-100) */
  overallProgress?: number;
  /** Optional CSS class name */
  className?: string;
}

/**
 * Props for missing pinned adapters banner
 */
export interface MissingPinnedAdaptersBannerProps {
  /** List of unavailable adapter names */
  unavailableAdapters: string[];
  /** Fallback strategy used */
  fallbackStrategy?: 'stack_only' | 'partial';
  /** Callback to load the missing adapters */
  onLoadAdapters?: () => void;
  /** Whether adapters are currently being loaded */
  isLoading?: boolean;
  /** Optional CSS class name */
  className?: string;
}
