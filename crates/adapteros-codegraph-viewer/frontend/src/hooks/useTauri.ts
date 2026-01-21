// Tauri command wrappers

import { invoke } from '@tauri-apps/api/tauri';
import type {
  GraphData,
  SymbolMatch,
  SymbolDetails,
  Neighbors,
  GraphDiffData,
} from '../types/graph';

export const useTauri = () => {
  const loadGraph = async (dbPath: string): Promise<GraphData> => {
    return await invoke<GraphData>('load_graph', { dbPath });
  };

  const searchSymbols = async (
    dbPath: string,
    query: string
  ): Promise<SymbolMatch[]> => {
    return await invoke<SymbolMatch[]>('search_symbols', { dbPath, query });
  };

  const getSymbolDetails = async (
    dbPath: string,
    symbolId: string
  ): Promise<SymbolDetails> => {
    return await invoke<SymbolDetails>('get_symbol_details', {
      dbPath,
      symbolId,
    });
  };

  const getNeighbors = async (
    dbPath: string,
    symbolId: string
  ): Promise<Neighbors> => {
    return await invoke<Neighbors>('get_neighbors', { dbPath, symbolId });
  };

  const loadDiff = async (
    dbPathA: string,
    dbPathB: string
  ): Promise<GraphDiffData> => {
    return await invoke<GraphDiffData>('load_diff', { dbPathA, dbPathB });
  };

  const openSourceFile = async (
    filePath: string,
    line: number
  ): Promise<void> => {
    return await invoke<void>('open_source_file', { filePath, line });
  };

  return {
    loadGraph,
    searchSymbols,
    getSymbolDetails,
    getNeighbors,
    loadDiff,
    openSourceFile,
  };
};

