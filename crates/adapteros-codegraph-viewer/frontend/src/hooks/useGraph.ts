// Graph data state management

import { useState, useCallback } from 'react';
import type { GraphData, GraphDiffData, SymbolDetails } from '../types/graph';

export const useGraph = () => {
  const [graphData, setGraphData] = useState<GraphData | null>(null);
  const [diffData, setDiffData] = useState<GraphDiffData | null>(null);
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [selectedDetails, setSelectedDetails] = useState<SymbolDetails | null>(
    null
  );
  const [dbPath, setDbPath] = useState<string>('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadGraph = useCallback((data: GraphData, path: string) => {
    setGraphData(data);
    setDbPath(path);
    setDiffData(null);
    setError(null);
  }, []);

  const loadDiff = useCallback((data: GraphDiffData) => {
    setDiffData(data);
    setError(null);
  }, []);

  const selectNode = useCallback((nodeId: string | null) => {
    setSelectedNode(nodeId);
  }, []);

  const setDetails = useCallback((details: SymbolDetails | null) => {
    setSelectedDetails(details);
  }, []);

  const clearGraph = useCallback(() => {
    setGraphData(null);
    setDiffData(null);
    setSelectedNode(null);
    setSelectedDetails(null);
    setDbPath('');
    setError(null);
  }, []);

  const setLoadingState = useCallback((loading: boolean) => {
    setIsLoading(loading);
  }, []);

  const setErrorState = useCallback((err: string | null) => {
    setError(err);
    setIsLoading(false);
  }, []);

  return {
    graphData,
    diffData,
    selectedNode,
    selectedDetails,
    dbPath,
    isLoading,
    error,
    loadGraph,
    loadDiff,
    selectNode,
    setDetails,
    clearGraph,
    setLoadingState,
    setErrorState,
  };
};

