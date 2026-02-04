// Main application component

import { useState, useCallback, useEffect } from 'react';
import { GraphCanvas } from './components/GraphCanvas';
import { Toolbar } from './components/Toolbar';
import { SearchBar } from './components/SearchBar';
import { SidePanel } from './components/SidePanel';
import { ColorLegend } from './components/ColorLegend';
import { DiffControls } from './components/DiffControls';
import { useGraph } from './hooks/useGraph';
import { useTauri } from './hooks/useTauri';
import { graphFixture, detailsFixture } from './testFixtures/graphFixture';
import type { LayoutType, SymbolMatch, GraphNode } from './types/graph';

function App() {
  const {
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
  } = useGraph();

  const tauri = useTauri();

  const [layoutType, setLayoutType] = useState<LayoutType>('cola');
  const [isDiffMode, setIsDiffMode] = useState(false);
  const [searchResults, setSearchResults] = useState<SymbolMatch[]>([]);
  const [isSearching, setIsSearching] = useState(false);

  // Diff mode state
  const [diffDbPathA, setDiffDbPathA] = useState<string | null>(null);
  const [diffDbPathB, setDiffDbPathB] = useState<string | null>(null);

  useEffect(() => {
    if (import.meta.env.VITE_CODEGRAPH_TEST_DATA !== '1') return;
    const params = new URLSearchParams(window.location.search);
    if (params.get('testData') !== '1') return;
    loadGraph(graphFixture, 'fixture.db');
    selectNode(detailsFixture.id);
    setDetails(detailsFixture);
  }, [loadGraph, selectNode, setDetails]);

  // Load graph from database
  const handleLoadGraph = useCallback(
    async (path: string) => {
      try {
        setLoadingState(true);
        const data = await tauri.loadGraph(path);
        loadGraph(data, path);
      } catch (err) {
        setErrorState(err instanceof Error ? err.message : String(err));
      } finally {
        setLoadingState(false);
      }
    },
    [tauri, loadGraph, setLoadingState, setErrorState]
  );

  // Search symbols
  const handleSearch = useCallback(
    async (query: string) => {
      if (!dbPath) return;

      try {
        setIsSearching(true);
        const results = await tauri.searchSymbols(dbPath, query);
        setSearchResults(results);
      } catch (err) {
        console.error('Search failed:', err);
        setSearchResults([]);
      } finally {
        setIsSearching(false);
      }
    },
    [dbPath, tauri]
  );

  // Select symbol from search results
  const handleSelectSearchResult = useCallback(
    async (symbolId: string) => {
      if (!dbPath) return;

      try {
        selectNode(symbolId);
        const details = await tauri.getSymbolDetails(dbPath, symbolId);
        setDetails(details);
      } catch (err) {
        console.error('Failed to get symbol details:', err);
      }
    },
    [dbPath, tauri, selectNode, setDetails]
  );

  // Handle node selection from graph
  const handleNodeSelect = useCallback(
    async (nodeId: string | null) => {
      selectNode(nodeId);

      if (nodeId && dbPath) {
        try {
          const details = await tauri.getSymbolDetails(dbPath, nodeId);
          setDetails(details);
        } catch (err) {
          console.error('Failed to get symbol details:', err);
        }
      } else {
        setDetails(null);
      }
    },
    [dbPath, tauri, selectNode, setDetails]
  );

  // Handle double click to open file
  const handleNodeDoubleClick = useCallback(
    async (node: GraphNode) => {
      try {
        await tauri.openSourceFile(node.file_path, node.span.start_line);
      } catch (err) {
        console.error('Failed to open file:', err);
      }
    },
    [tauri]
  );

  // Load diff between two databases
  const handleLoadDiff = useCallback(async () => {
    if (!diffDbPathA || !diffDbPathB) return;

    try {
      setLoadingState(true);
      const diffResult = await tauri.loadDiff(diffDbPathA, diffDbPathB);
      loadDiff(diffResult);
    } catch (err) {
      setErrorState(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingState(false);
    }
  }, [diffDbPathA, diffDbPathB, tauri, loadDiff, setLoadingState, setErrorState]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'o') {
        e.preventDefault();
        // Trigger file open - handled by Toolbar
      } else if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
        e.preventDefault();
        // Focus search bar
        const searchInput = document.querySelector('input[placeholder*="Search"]') as HTMLInputElement;
        searchInput?.focus();
      } else if ((e.metaKey || e.ctrlKey) && e.key === 'd') {
        e.preventDefault();
        setIsDiffMode((prev) => !prev);
      } else if (e.key === 'Escape') {
        selectNode(null);
        setDetails(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectNode, setDetails]);

  return (
    <div style={appStyle}>
      <Toolbar
        onLoadGraph={handleLoadGraph}
        onToggleDiffMode={() => setIsDiffMode((prev) => !prev)}
        onClearGraph={clearGraph}
        onResetLayout={() => setLayoutType(layoutType)}
        layoutType={layoutType}
        onLayoutChange={setLayoutType}
        isDiffMode={isDiffMode}
        hasGraph={graphData !== null}
      />

      <div style={mainContainerStyle}>
        <div style={searchContainerStyle}>
          {graphData && (
            <SearchBar
              onSearch={handleSearch}
              onSelectResult={handleSelectSearchResult}
              results={searchResults}
              isLoading={isSearching}
            />
          )}
        </div>

        <div style={canvasContainerStyle}>
          {isLoading && (
            <div style={loadingStyle}>
              <div style={{ fontSize: '48px' }}>⏳</div>
              <div style={{ marginTop: '16px', fontSize: '18px', color: 'white' }}>
                Loading graph...
              </div>
            </div>
          )}

          {error && (
            <div style={errorStyle}>
              <div style={{ fontSize: '48px' }}>⚠️</div>
              <div style={{ marginTop: '16px', fontSize: '18px', color: 'white' }}>
                Error: {error}
              </div>
            </div>
          )}

          {!isLoading && !error && !graphData && (
            <div style={emptyStateStyle}>
              <div style={{ fontSize: '64px' }}>📊</div>
              <div style={{ marginTop: '16px', fontSize: '24px', color: 'white' }}>
                CodeGraph Viewer
              </div>
              <div style={{ marginTop: '8px', fontSize: '16px', color: '#9ca3af' }}>
                Open a CodeGraph database to visualize code relationships
              </div>
              <div style={{ marginTop: '16px', fontSize: '14px', color: '#6b7280' }}>
                Press <kbd style={kbdStyle}>Cmd+O</kbd> to open a database
              </div>
            </div>
          )}

          {graphData && (
            <>
              <GraphCanvas
                graphData={graphData}
                diffData={diffData}
                selectedNode={selectedNode}
                layoutType={layoutType}
                onNodeSelect={handleNodeSelect}
                onNodeDoubleClick={handleNodeDoubleClick}
              />
              <ColorLegend showDiffLegend={diffData !== null} />
            </>
          )}
        </div>

        {selectedDetails && (
          <SidePanel
            details={selectedDetails}
            onClose={() => {
              selectNode(null);
              setDetails(null);
            }}
            onOpenFile={async (filePath, line) => {
              try {
                await tauri.openSourceFile(filePath, line);
              } catch (err) {
                console.error('Failed to open file:', err);
              }
            }}
            onSelectSymbol={handleSelectSearchResult}
          />
        )}

        {isDiffMode && (
          <DiffControls
            isActive={isDiffMode}
            dbPathA={diffDbPathA}
            dbPathB={diffDbPathB}
            diffStats={diffData?.stats || null}
            onSetDbPathA={setDiffDbPathA}
            onSetDbPathB={setDiffDbPathB}
            onLoadDiff={handleLoadDiff}
            onClose={() => setIsDiffMode(false)}
          />
        )}
      </div>
    </div>
  );
}

const appStyle: React.CSSProperties = {
  width: '100vw',
  height: '100vh',
  display: 'flex',
  flexDirection: 'column',
  overflow: 'hidden',
  backgroundColor: '#1a1a1a',
  fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
};

const mainContainerStyle: React.CSSProperties = {
  flex: 1,
  display: 'flex',
  position: 'relative',
  overflow: 'hidden',
};

const searchContainerStyle: React.CSSProperties = {
  position: 'absolute',
  top: '20px',
  left: '20px',
  zIndex: 1000,
};

const canvasContainerStyle: React.CSSProperties = {
  flex: 1,
  position: 'relative',
};

const loadingStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
};

const errorStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
  color: '#ef4444',
};

const emptyStateStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  height: '100%',
  textAlign: 'center',
};

const kbdStyle: React.CSSProperties = {
  padding: '4px 8px',
  backgroundColor: '#374151',
  borderRadius: '4px',
  fontSize: '12px',
  fontWeight: '600',
  color: '#d1d5db',
  border: '1px solid #4b5563',
};

export default App;
