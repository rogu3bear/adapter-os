// Side panel showing selected node details

import type { SymbolDetails } from '../types/graph';

interface SidePanelProps {
  details: SymbolDetails | null;
  onClose: () => void;
  onOpenFile: (filePath: string, line: number) => void;
  onSelectSymbol: (symbolId: string) => void;
}

export const SidePanel = ({
  details,
  onClose,
  onOpenFile,
  onSelectSymbol,
}: SidePanelProps) => {
  if (!details) return null;

  const getBadgeStyle = (color: string): React.CSSProperties => ({
    display: 'inline-block',
    padding: '4px 8px',
    backgroundColor: color,
    color: 'white',
    borderRadius: '4px',
    fontSize: '12px',
    fontWeight: '500',
  });

  return (
    <div style={panelStyle}>
      <div style={headerStyle}>
        <h3 style={{ margin: 0, color: 'white' }}>Symbol Details</h3>
        <button onClick={onClose} style={closeButtonStyle}>
          ✕
        </button>
      </div>

      <div style={contentStyle}>
        <section style={sectionStyle}>
          <h4 style={sectionTitleStyle}>Name</h4>
          <div
            style={{
              fontSize: '18px',
              fontWeight: '600',
              color: 'white',
              wordBreak: 'break-word',
              overflowWrap: 'anywhere',
            }}
          >
            {details.name}
          </div>
          {details.qualified_name !== details.name && (
            <div
              style={{
                fontSize: '14px',
                color: '#9ca3af',
                marginTop: '4px',
                wordBreak: 'break-word',
                overflowWrap: 'anywhere',
              }}
            >
              {details.qualified_name}
            </div>
          )}
        </section>

        <section style={sectionStyle}>
          <h4 style={sectionTitleStyle}>Kind</h4>
          <span style={getBadgeStyle('#3b82f6')}>{details.kind}</span>
        </section>

        <section style={sectionStyle}>
          <h4 style={sectionTitleStyle}>Location</h4>
          <div
            style={{
              color: '#60a5fa',
              cursor: 'pointer',
              textDecoration: 'underline',
              wordBreak: 'break-word',
              overflowWrap: 'anywhere',
            }}
            onClick={() => onOpenFile(details.file_path, details.span.start_line)}
            title={`${details.file_path}:${details.span.start_line}`}
          >
            {details.file_path}:{details.span.start_line}
          </div>
          <div style={{ fontSize: '12px', color: '#9ca3af', marginTop: '4px' }}>
            Lines {details.span.start_line}-{details.span.end_line}
          </div>
        </section>

        <section style={sectionStyle}>
          <h4 style={sectionTitleStyle}>Visibility</h4>
          <span style={getBadgeStyle('#8b5cf6')}>{details.visibility}</span>
        </section>

        {(details.is_recursive || details.is_async || details.is_unsafe) && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Flags</h4>
            <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
              {details.is_recursive && <span style={getBadgeStyle('#ef4444')}>Recursive</span>}
              {details.is_async && <span style={getBadgeStyle('#10b981')}>Async</span>}
              {details.is_unsafe && <span style={getBadgeStyle('#f59e0b')}>Unsafe</span>}
            </div>
          </section>
        )}

        {details.type_annotation && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Type</h4>
            <code style={codeStyle}>{details.type_annotation}</code>
          </section>
        )}

        {details.signature && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Signature</h4>
            <code style={codeStyle}>{details.signature}</code>
          </section>
        )}

        {details.docstring && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Documentation</h4>
            <div
              style={{
                color: '#d1d5db',
                fontSize: '14px',
                lineHeight: '1.5',
                wordBreak: 'break-word',
                overflowWrap: 'anywhere',
              }}
            >
              {details.docstring}
            </div>
          </section>
        )}

        {details.callers.length > 0 && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Callers ({details.callers.length})</h4>
            <div style={listStyle}>
              {details.callers.map((caller) => (
                <div
                  key={caller.id}
                  style={listItemStyle}
                  onClick={() => onSelectSymbol(caller.id)}
                >
                  <span style={{ fontSize: '14px' }}>→</span>
                  <span style={{ fontWeight: '500' }}>{caller.name}</span>
                  <span style={{ fontSize: '12px', color: '#9ca3af' }}>
                    ({caller.kind})
                  </span>
                </div>
              ))}
            </div>
          </section>
        )}

        {details.callees.length > 0 && (
          <section style={sectionStyle}>
            <h4 style={sectionTitleStyle}>Callees ({details.callees.length})</h4>
            <div style={listStyle}>
              {details.callees.map((callee) => (
                <div
                  key={callee.id}
                  style={listItemStyle}
                  onClick={() => onSelectSymbol(callee.id)}
                >
                  <span style={{ fontSize: '14px' }}>→</span>
                  <span style={{ fontWeight: '500' }}>{callee.name}</span>
                  <span style={{ fontSize: '12px', color: '#9ca3af' }}>
                    ({callee.kind})
                  </span>
                </div>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
};

const panelStyle: React.CSSProperties = {
  width: '350px',
  height: '100%',
  backgroundColor: '#1f2937',
  borderLeft: '1px solid #374151',
  display: 'flex',
  flexDirection: 'column',
  overflow: 'hidden',
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
  fontSize: '20px',
  cursor: 'pointer',
  padding: '4px 8px',
};

const contentStyle: React.CSSProperties = {
  flex: 1,
  overflow: 'auto',
  padding: '16px',
};

const sectionStyle: React.CSSProperties = {
  marginBottom: '20px',
};

const sectionTitleStyle: React.CSSProperties = {
  margin: '0 0 8px 0',
  fontSize: '12px',
  fontWeight: '600',
  textTransform: 'uppercase',
  color: '#9ca3af',
};

const codeStyle: React.CSSProperties = {
  display: 'block',
  padding: '8px 12px',
  backgroundColor: '#111827',
  borderRadius: '4px',
  fontSize: '13px',
  color: '#10b981',
  fontFamily: 'monospace',
  overflowX: 'auto',
};

const listStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
};

const listItemStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  padding: '8px 12px',
  backgroundColor: '#374151',
  borderRadius: '4px',
  cursor: 'pointer',
  color: 'white',
  transition: 'background-color 0.2s',
  wordBreak: 'break-word',
  overflowWrap: 'anywhere',
};
