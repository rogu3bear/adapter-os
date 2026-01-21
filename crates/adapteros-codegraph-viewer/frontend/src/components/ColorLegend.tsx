// Color legend showing node types and diff states

import { NODE_COLORS, EDGE_COLORS, DIFF_COLORS } from '../utils/colors';

interface ColorLegendProps {
  showDiffLegend: boolean;
}

export const ColorLegend = ({ showDiffLegend }: ColorLegendProps) => {
  return (
    <div style={containerStyle}>
      <h4 style={titleStyle}>Node Types</h4>
      <div style={legendGroupStyle}>
        <LegendItem color={NODE_COLORS.function} label="Function" />
        <LegendItem color={NODE_COLORS.struct} label="Struct" />
        <LegendItem color={NODE_COLORS.trait} label="Trait" />
        <LegendItem color={NODE_COLORS.impl} label="Impl" />
        <LegendItem color={NODE_COLORS.module} label="Module" />
        <LegendItem color={NODE_COLORS.enum} label="Enum" />
        <LegendItem color={NODE_COLORS.method} label="Method" />
      </div>

      <h4 style={titleStyle}>Edge Types</h4>
      <div style={legendGroupStyle}>
        <LegendItem color={EDGE_COLORS.normal} label="Normal Call" />
        <LegendItem color={EDGE_COLORS.recursive} label="Recursive" />
        <LegendItem color={EDGE_COLORS.trait_call} label="Trait Call" dashed />
      </div>

      {showDiffLegend && (
        <>
          <h4 style={titleStyle}>Diff States</h4>
          <div style={legendGroupStyle}>
            <LegendItem color={DIFF_COLORS.added} label="Added" />
            <LegendItem color={DIFF_COLORS.removed} label="Removed" />
            <LegendItem color={DIFF_COLORS.modified} label="Modified" />
            <LegendItem color={DIFF_COLORS.unchanged} label="Unchanged" />
          </div>
        </>
      )}
    </div>
  );
};

interface LegendItemProps {
  color: string;
  label: string;
  dashed?: boolean;
}

const LegendItem = ({ color, label, dashed }: LegendItemProps) => (
  <div style={itemStyle}>
    <div
      style={{
        width: '20px',
        height: dashed ? '2px' : '12px',
        backgroundColor: color,
        borderRadius: dashed ? '0' : '2px',
        border: dashed ? `2px dashed ${color}` : 'none',
      }}
    />
    <span style={labelStyle}>{label}</span>
  </div>
);

const containerStyle: React.CSSProperties = {
  position: 'absolute',
  bottom: '20px',
  left: '20px',
  backgroundColor: 'rgba(26, 26, 26, 0.95)',
  border: '1px solid #374151',
  borderRadius: '8px',
  padding: '16px',
  minWidth: '180px',
  zIndex: 1000,
  backdropFilter: 'blur(10px)',
};

const titleStyle: React.CSSProperties = {
  margin: '0 0 8px 0',
  fontSize: '12px',
  fontWeight: '600',
  textTransform: 'uppercase',
  color: '#9ca3af',
};

const legendGroupStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '6px',
  marginBottom: '16px',
};

const itemStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
};

const labelStyle: React.CSSProperties = {
  fontSize: '13px',
  color: '#d1d5db',
};

