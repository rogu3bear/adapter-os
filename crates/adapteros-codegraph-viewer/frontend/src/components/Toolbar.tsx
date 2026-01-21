// Top toolbar component

import { open } from '@tauri-apps/api/dialog';
import type { LayoutType } from '../types/graph';

interface ToolbarProps {
  onLoadGraph: (path: string) => void;
  onToggleDiffMode: () => void;
  onClearGraph: () => void;
  onResetLayout: () => void;
  layoutType: LayoutType;
  onLayoutChange: (layout: LayoutType) => void;
  isDiffMode: boolean;
  hasGraph: boolean;
}

export const Toolbar = ({
  onLoadGraph,
  onToggleDiffMode,
  onClearGraph,
  onResetLayout,
  layoutType,
  onLayoutChange,
  isDiffMode,
  hasGraph,
}: ToolbarProps) => {
  const handleOpenDatabase = async () => {
    try {
      const selected = await open({
        title: 'Open CodeGraph Database',
        filters: [
          {
            name: 'SQLite Database',
            extensions: ['db', 'sqlite', 'sqlite3'],
          },
        ],
      });

      if (selected && typeof selected === 'string') {
        onLoadGraph(selected);
      }
    } catch (error) {
      console.error('Failed to open file dialog:', error);
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '12px',
        padding: '12px 16px',
        backgroundColor: '#2a2a2a',
        borderBottom: '1px solid #3a3a3a',
      }}
    >
      <button
        onClick={handleOpenDatabase}
        style={buttonStyle}
        title="Open Database (Cmd+O)"
      >
        📂 Open Database
      </button>

      {hasGraph && (
        <>
          <button
            onClick={onToggleDiffMode}
            style={{
              ...buttonStyle,
              backgroundColor: isDiffMode ? '#3b82f6' : '#4a5568',
            }}
            title="Toggle Diff Mode (Cmd+D)"
          >
            {isDiffMode ? '🔄 Diff Mode: ON' : '🔄 Diff Mode'}
          </button>

          <select
            value={layoutType}
            onChange={(e) => onLayoutChange(e.target.value as LayoutType)}
            style={selectStyle}
            title="Layout Algorithm"
          >
            <option value="cola">Force-Directed (Cola)</option>
            <option value="dagre">Hierarchical (Dagre)</option>
            <option value="circle">Circular</option>
            <option value="grid">Grid</option>
          </select>

          <button
            onClick={onResetLayout}
            style={buttonStyle}
            title="Reset Layout (Cmd+R)"
          >
            🔄 Reset Layout
          </button>

          <button
            onClick={onClearGraph}
            style={buttonStyle}
            title="Clear Graph"
          >
            ❌ Clear
          </button>
        </>
      )}

      <div style={{ marginLeft: 'auto', color: '#9ca3af', fontSize: '14px' }}>
        CodeGraph Viewer v0.1.0
      </div>
    </div>
  );
};

const buttonStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#4a5568',
  color: 'white',
  border: 'none',
  borderRadius: '4px',
  cursor: 'pointer',
  fontSize: '14px',
  fontWeight: '500',
  transition: 'background-color 0.2s',
};

const selectStyle: React.CSSProperties = {
  padding: '8px 12px',
  backgroundColor: '#374151',
  color: 'white',
  border: '1px solid #4b5563',
  borderRadius: '4px',
  cursor: 'pointer',
  fontSize: '14px',
};

