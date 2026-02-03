// Symbol search component

import { useState, useEffect, useRef } from 'react';
import type { SymbolMatch } from '../types/graph';

interface SearchBarProps {
  onSearch: (query: string) => void;
  onSelectResult: (symbolId: string) => void;
  results: SymbolMatch[];
  isLoading: boolean;
}

export const SearchBar = ({
  onSearch,
  onSelectResult,
  results,
  isLoading,
}: SearchBarProps) => {
  const [query, setQuery] = useState('');
  const [showResults, setShowResults] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Debounced search
  useEffect(() => {
    const timer = setTimeout(() => {
      if (query.length >= 2) {
        onSearch(query);
        setShowResults(true);
      } else {
        setShowResults(false);
      }
    }, 300);

    return () => clearTimeout(timer);
  }, [query, onSearch]);

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setShowResults(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleSelectResult = (symbolId: string) => {
    onSelectResult(symbolId);
    setShowResults(false);
    setQuery('');
  };

  const getKindIcon = (kind: string): string => {
    const kindLower = kind.toLowerCase();
    if (kindLower.includes('function')) return '⚡';
    if (kindLower.includes('struct')) return '📦';
    if (kindLower.includes('trait')) return '🔷';
    if (kindLower.includes('impl')) return '🔶';
    if (kindLower.includes('module')) return '📁';
    if (kindLower.includes('enum')) return '🔢';
    return '📄';
  };

  return (
    <div ref={containerRef} style={containerStyle}>
      <div style={searchBoxStyle}>
        <span style={{ fontSize: '16px' }}>🔍</span>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search symbols... (Cmd+F)"
          style={inputStyle}
        />
        {isLoading && (
          <span style={{ fontSize: '14px', color: '#9ca3af' }}>⏳</span>
        )}
      </div>

      {showResults && results.length > 0 && (
        <div style={resultsStyle}>
          {results.map((result) => (
            <div
              key={result.id}
              onClick={() => handleSelectResult(result.id)}
              style={resultItemStyle}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <span style={{ fontSize: '16px' }}>{getKindIcon(result.kind)}</span>
                <div>
                  <div style={resultTitleStyle}>{result.name}</div>
                  <div style={resultMetaStyle} title={`${result.kind} • ${result.file_path}`}>
                    {result.kind} • {result.file_path}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {showResults && results.length === 0 && !isLoading && query.length >= 2 && (
        <div style={resultsStyle}>
          <div style={{ padding: '12px', color: '#9ca3af', textAlign: 'center' }}>
            No results found
          </div>
        </div>
      )}
    </div>
  );
};

const containerStyle: React.CSSProperties = {
  position: 'relative',
  width: '400px',
  maxWidth: 'calc(100vw - 40px)',
};

const searchBoxStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '8px 12px',
  backgroundColor: '#374151',
  border: '1px solid #4b5563',
  borderRadius: '4px',
};

const inputStyle: React.CSSProperties = {
  flex: 1,
  backgroundColor: 'transparent',
  border: 'none',
  outline: 'none',
  color: 'white',
  fontSize: '14px',
};

const resultsStyle: React.CSSProperties = {
  position: 'absolute',
  top: 'calc(100% + 4px)',
  left: 0,
  right: 0,
  backgroundColor: '#2a2a2a',
  border: '1px solid #4b5563',
  borderRadius: '4px',
  maxHeight: '400px',
  overflowY: 'auto',
  zIndex: 1000,
  boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
};

const resultItemStyle: React.CSSProperties = {
  padding: '12px',
  cursor: 'pointer',
  borderBottom: '1px solid #3a3a3a',
  transition: 'background-color 0.2s',
};

const resultTitleStyle: React.CSSProperties = {
  fontWeight: '500',
  color: 'white',
  wordBreak: 'break-word',
  overflowWrap: 'anywhere',
};

const resultMetaStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#9ca3af',
  wordBreak: 'break-word',
  overflowWrap: 'anywhere',
};
