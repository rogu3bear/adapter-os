import { useMemo } from 'react';
import type { BaseModelStatus } from '@/api/types';
import { useDemoMode } from './DemoProvider';

export function useDemoBaseModelStatus(status: BaseModelStatus | null | undefined): BaseModelStatus | null | undefined {
  const { enabled, activeModel, modelSwitching } = useDemoMode();

  return useMemo(() => {
    if (!enabled || !activeModel) return status;
    return {
      ...status,
      model_id: activeModel.id,
      model_name: activeModel.name,
      status: modelSwitching ? 'loading' : 'ready',
      memory_usage_mb: activeModel.memoryUsageMb ?? status?.memory_usage_mb,
      model_path: status?.model_path ?? activeModel.backend,
    } as BaseModelStatus;
  }, [activeModel, enabled, modelSwitching, status]);
}
