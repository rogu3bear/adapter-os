import { useMemo, useState } from 'react';
import type { Adapter } from '@/api/types';
import { FilterConfig, type FilterValues } from '@/components/ui/advanced-filter';

export function useAdapterFilters(adapters: Adapter[]) {
  const [filterValues, setFilterValues] = useState<FilterValues>({});

  const adapterFilterConfigs: FilterConfig[] = useMemo(() => [
    {
      id: 'search',
      label: 'Search',
      type: 'text',
      placeholder: 'Search by name or adapter ID...',
    },
    {
      id: 'category',
      label: 'Category',
      type: 'select',
      options: [
        { value: 'code', label: 'Code' },
        { value: 'framework', label: 'Framework' },
        { value: 'codebase', label: 'Codebase' },
        { value: 'ephemeral', label: 'Ephemeral' },
      ],
    },
    {
      id: 'framework',
      label: 'Framework',
      type: 'select',
      options: Array.from(new Set(adapters.map(a => a.framework).filter(Boolean)))
        .map(f => ({ value: f!, label: f! })),
    },
    {
      id: 'state',
      label: 'State',
      type: 'multiSelect',
      options: [
        { value: 'unloaded', label: 'Unloaded' },
        { value: 'cold', label: 'Cold' },
        { value: 'warm', label: 'Warm' },
        { value: 'hot', label: 'Hot' },
        { value: 'resident', label: 'Resident' },
      ],
    },
    {
      id: 'tier',
      label: 'Tier',
      type: 'multiSelect',
      options: [
        { value: '1', label: 'Tier 1' },
        { value: '2', label: 'Tier 2' },
        { value: '3', label: 'Tier 3' },
        { value: '4', label: 'Tier 4' },
      ],
    },
    {
      id: 'scope',
      label: 'Scope',
      type: 'multiSelect',
      options: [
        { value: 'global', label: 'Global' },
        { value: 'tenant', label: 'Workspace' },
        { value: 'repo', label: 'Repo' },
        { value: 'commit', label: 'Commit' },
      ],
    },
    {
      id: 'pinned',
      label: 'Protected Only',
      type: 'toggle',
    },
  ], [adapters]);

  const filteredAdapters = useMemo(() => adapters.filter((adapter) => {
    if (filterValues.search) {
      const searchLower = String(filterValues.search).toLowerCase();
      if (
        !adapter.name.toLowerCase().includes(searchLower) &&
        !adapter.adapter_id.toLowerCase().includes(searchLower) &&
        !(adapter.framework?.toLowerCase().includes(searchLower))
      ) {
        return false;
      }
    }
    if (filterValues.category && adapter.category !== filterValues.category) {
      return false;
    }
    if (filterValues.framework && adapter.framework !== filterValues.framework) {
      return false;
    }
    if (filterValues.state && Array.isArray(filterValues.state) && filterValues.state.length > 0) {
      if (!adapter.current_state || !filterValues.state.includes(adapter.current_state)) {
        return false;
      }
    }
    if (filterValues.tier && Array.isArray(filterValues.tier) && filterValues.tier.length > 0) {
      if (!adapter.tier || !filterValues.tier.includes(String(adapter.tier))) {
        return false;
      }
    }
    if (filterValues.scope && Array.isArray(filterValues.scope) && filterValues.scope.length > 0) {
      if (!adapter.scope || !filterValues.scope.includes(adapter.scope)) {
        return false;
      }
    }
    if (filterValues.pinned === true && !adapter.pinned) {
      return false;
    }
    return true;
  }), [adapters, filterValues]);

  return { adapterFilterConfigs, filteredAdapters, filterValues, setFilterValues };
}
