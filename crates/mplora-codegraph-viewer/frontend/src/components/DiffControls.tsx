// Diff mode controls for comparing commits

import { open } from '@tauri-apps/api/dialog';
import type { DiffStats } from '../types/graph';

interface DiffControlsProps {
  isActive: boolean;
  dbPathA: string | null;
  dbPathB: string | null;
  diffStats: DiffStats | null;
  onSetDbPathA: (path: string) => void;
  onSetDbPathB: (path: string) => void;
  onLoadDiff: () => void;
  onClose: () => void;
}

export const DiffControls = ({
  isActive,
  dbPathA,
  dbPathB,
  diffStats,
  onSetDbPathA,
  onSetDbPathB,
  onLoadDiff,
  onClose,
}: DiffControlsProps) => {
  if (!isActive) return null;

  const handleSelectDbA = async () => {
    try {
      const selected = await open({
        title: 'Select Base Commit Database',
        filters: [
          {
            name: 'SQLite Database',
            extensions: ['db', 'sqlite', 'sqlite3'],
          },
        ],
      });

      if (selected && typeof selected === 'string') {
        onSetDbPathA(selected);
      }
    } catch (error) {
      console.error('Failed to open file dialog:', error);
    }
  };

  const handleSelectDbB = async () => {
    try {
      const selected = await open({
        title: 'Select Compare Commit Database',
        filters: [
          {
            name: 'SQLite Database',
            extensions: ['db', 'sqlite', 'sqlite3'],
          },
        ],
      });

      if (selected && typeof selected === 'string') {
        onSetDbPathB(selected);
      }
    } catch (error) {
      console.error('Failed to open file dialog:', error);
    }
  };

  const canLoadDiff = dbPathA && dbPathB;

  return (
    <div style={containerStyle}>
      <div style={headerStyle}>
        <h3 style={{ margin: 0, color: 'white', fontSize: '16px' }}>
          Diff Mode
        </h3>
        <button onClick={onClose} style={closeButtonStyle}>
          ✕
        </button>
      </div>

      <div style={contentStyle}>
        <div style={sectionStyle}>
          <label style={labelStyle}>Base Commit (A)</label>
          <div style={filePickerStyle}>
            <button onClick={handleSelectDbA} style={buttonStyle}>
              📂 Select Database
            </button>
            {dbPathA && (
              <div style={pathDisplayStyle}>{getFileName(dbPathA)}</div>
            )}
          </div>
        </div>

        <div style={sectionStyle}>
          <label style={labelStyle}>Compare Commit (B)</label>
          <div style={filePickerStyle}>
            <button onClick={handleSelectDbB} style={buttonStyle}>
              📂 Select Database
            </button>
            {dbPathB && (
              <div style={pathDisplayStyle}>{getFileName(dbPathB)}</div>
            )}
          </div>
        </div>

        <button
          onClick={onLoadDiff}
          disabled={!canLoadDiff}
          style={{
            ...loadButtonStyle,
            opacity: canLoadDiff ? 1 : 0.5,
            cursor: canLoadDiff ? 'pointer' : 'not-allowed',
          }}
        >
          🔄 Load Diff
        </button>

        {diffStats && (
          <div style={statsStyle}>
            <h4 style={statsTitle}>Diff Statistics</h4>
            <div style={statsList}>
              <StatItem
                label="Nodes Added"
                value={diffStats.nodes_added}
                color="#10b981"
              />
              <StatItem
                label="Nodes Removed"
                value={diffStats.nodes_removed}
                color="#ef4444"
              />
              <StatItem
                label="Nodes Modified"
                value={diffStats.nodes_modified}
                color="#f59e0b"
              />
              <StatItem
                label="Edges Added"
                value={diffStats.edges_added}
                color="#10b981"
              />
              <StatItem
                label="Edges Removed"
                value={diffStats.edges_removed}
                color="#ef4444"
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

interface StatItemProps {
  label: string;
  value: number;
  color: string;
}

const StatItem = ({ label, value, color }: StatItemProps) => (
  <div style={statItemStyle}>
    <span style={{ color: '#9ca3af', fontSize: '13px' }}>{label}</span>
    <span style={{ color, fontSize: '16px', fontWeight: '600' }}>{value}</span>
  </div>
);

const getFileName = (path: string): string => {
  const parts = path.split('/');
  return parts[parts.length - 1];
};

const containerStyle: React.CSSProperties = {
  position: 'absolute',
  top: '70px',
  right: '20px',
  width: '350px',
  backgroundColor: 'rgba(31, 41, 55, 0.98)',
  border: '1px solid #374151',
  borderRadius: '8px',
  boxShadow: '0 10px 25px rgba(0, 0, 0, 0.5)',
  zIndex: 1000,
  backdropFilter: 'blur(10px)',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '16px',
  borderBottom: '1px solid #374151',
};

const closeButtonStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  color: '#9ca3af',
  fontSize: '18px',
  cursor: 'pointer',
  padding: '4px',
};

const contentStyle: React.CSSProperties = {
  padding: '16px',
};

const sectionStyle: React.CSSProperties = {
  marginBottom: '16px',
};

const labelStyle: React.CSSProperties = {
  display: 'block',
  marginBottom: '8px',
  fontSize: '13px',
  fontWeight: '500',
  color: '#d1d5db',
};

const filePickerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
};

const buttonStyle: React.CSSProperties = {
  padding: '8px 12px',
  backgroundColor: '#374151',
  color: 'white',
  border: '1px solid #4b5563',
  borderRadius: '4px',
  cursor: 'pointer',
  fontSize: '13px',
  fontWeight: '500',
};

const pathDisplayStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#9ca3af',
  padding: '4px 8px',
  backgroundColor: '#1f2937',
  borderRadius: '4px',
  overflow: 'hidden',
  textOverflow: 'ellipsis',
  whiteSpace: 'nowrap',
};

const loadButtonStyle: React.CSSProperties = {
  width: '100%',
  padding: '10px',
  backgroundColor: '#3b82f6',
  color: 'white',
  border: 'none',
  borderRadius: '4px',
  fontSize: '14px',
  fontWeight: '600',
  marginTop: '8px',
};

const statsStyle: React.CSSProperties = {
  marginTop: '20px',
  padding: '12px',
  backgroundColor: '#1f2937',
  borderRadius: '6px',
  border: '1px solid #374151',
};

const statsTitle: React.CSSProperties = {
  margin: '0 0 12px 0',
  fontSize: '13px',
  fontWeight: '600',
  color: '#d1d5db',
};

const statsList: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
};

const statItemStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
};

