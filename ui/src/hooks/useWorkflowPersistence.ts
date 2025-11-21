// useWorkflowPersistence hook - Persist and resume workflow state

import { useState, useEffect, useCallback } from 'react';
import { SavedWorkflowState, WorkflowExecution } from '../components/workflows/types';
import { logger } from '../utils/logger';

interface UseWorkflowPersistenceOptions {
  storageKey: string;
  autoSave?: boolean;
  saveInterval?: number; // milliseconds
}

export function useWorkflowPersistence(options: UseWorkflowPersistenceOptions) {
  const { storageKey, autoSave = true, saveInterval = 5000 } = options;
  const [savedState, setSavedState] = useState<SavedWorkflowState | null>(null);
  const [executions, setExecutions] = useState<WorkflowExecution[]>([]);

  // Load saved state on mount
  useEffect(() => {
    loadSavedState();
    loadExecutions();
  }, [storageKey]);

  const loadSavedState = useCallback(() => {
    try {
      const saved = localStorage.getItem(`workflow-state-${storageKey}`);
      if (saved) {
        const state = JSON.parse(saved) as SavedWorkflowState;
        setSavedState(state);
        return state;
      }
    } catch (error) {
      logger.error('Failed to load saved workflow state', { error, component: 'useWorkflowPersistence' });
    }
    return null;
  }, [storageKey]);

  const saveState = useCallback(
    (state: SavedWorkflowState) => {
      try {
        localStorage.setItem(`workflow-state-${storageKey}`, JSON.stringify(state));
        setSavedState(state);
      } catch (error) {
        logger.error('Failed to save workflow state', { error, component: 'useWorkflowPersistence' });
      }
    },
    [storageKey]
  );

  const clearState = useCallback(() => {
    try {
      localStorage.removeItem(`workflow-state-${storageKey}`);
      setSavedState(null);
    } catch (error) {
      logger.error('Failed to clear workflow state', { error, component: 'useWorkflowPersistence' });
    }
  }, [storageKey]);

  const loadExecutions = useCallback(() => {
    try {
      const saved = localStorage.getItem(`workflow-executions`);
      if (saved) {
        const execs = JSON.parse(saved) as WorkflowExecution[];
        setExecutions(execs);
        return execs;
      }
    } catch (error) {
      logger.error('Failed to load workflow executions', { error, component: 'useWorkflowPersistence' });
    }
    return [];
  }, []);

  const saveExecution = useCallback((execution: WorkflowExecution) => {
    try {
      const current = localStorage.getItem(`workflow-executions`);
      const execs = current ? (JSON.parse(current) as WorkflowExecution[]) : [];

      // Add new execution at the beginning
      const updated = [execution, ...execs];

      // Keep only last 100 executions
      const trimmed = updated.slice(0, 100);

      localStorage.setItem(`workflow-executions`, JSON.stringify(trimmed));
      setExecutions(trimmed);
    } catch (error) {
      logger.error('Failed to save workflow execution', { error, component: 'useWorkflowPersistence' });
    }
  }, []);

  const deleteExecution = useCallback((executionId: string) => {
    try {
      const current = localStorage.getItem(`workflow-executions`);
      if (current) {
        const execs = JSON.parse(current) as WorkflowExecution[];
        const filtered = execs.filter((e) => e.id !== executionId);
        localStorage.setItem(`workflow-executions`, JSON.stringify(filtered));
        setExecutions(filtered);
      }
    } catch (error) {
      logger.error('Failed to delete workflow execution', { error, component: 'useWorkflowPersistence' });
    }
  }, []);

  const clearExecutions = useCallback(() => {
    try {
      localStorage.removeItem(`workflow-executions`);
      setExecutions([]);
    } catch (error) {
      logger.error('Failed to clear workflow executions', { error, component: 'useWorkflowPersistence' });
    }
  }, []);

  const hasSavedState = savedState !== null;

  return {
    savedState,
    saveState,
    clearState,
    loadSavedState,
    hasSavedState,
    executions,
    saveExecution,
    deleteExecution,
    clearExecutions,
    loadExecutions,
  };
}
