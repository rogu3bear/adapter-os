/**
 * Example usage patterns for diff utilities
 *
 * Demonstrates integration with React components and real-world scenarios
 */

import React, { useMemo, useState } from 'react';
import {
  tokenDiff,
  charDiff,
  lineDiff,
  similarity,
  getDiffStats,
  mergeDiffs,
  chunkDiffs,
  DiffResult,
  LineDiff,
} from './diff';

/**
 * Example 1: Simple Similarity Check
 *
 * Use case: Validation, duplicate detection, search relevance
 */
export function SimilarityCheckExample() {
  const [text1, setText1] = useState('hello world');
  const [text2, setText2] = useState('hello mars');

  const score = useMemo(() => similarity(text1, text2), [text1, text2]);

  return (
    <div>
      <h3>Similarity Check</h3>
      <input value={text1} onChange={(e) => setText1(e.target.value)} />
      <input value={text2} onChange={(e) => setText2(e.target.value)} />
      <p>Similarity: {score.toFixed(1)}%</p>
    </div>
  );
}

/**
 * Example 2: Line-by-Line Diff Display
 *
 * Use case: Code review, document comparison, version diffs
 */
export function LineDiffDisplayExample() {
  const [golden, setGolden] = useState('line1\nline2\nline3');
  const [current, setCurrent] = useState('line1\nmodified\nline3');

  const diffs = useMemo(() => lineDiff(golden, current, true), [golden, current]);

  return (
    <div>
      <h3>Line Diff Display</h3>
      <div style={{ display: 'flex', gap: '1rem' }}>
        <textarea value={golden} onChange={(e) => setGolden(e.target.value)} />
        <textarea value={current} onChange={(e) => setCurrent(e.target.value)} />
      </div>
      <div style={{ marginTop: '1rem', fontFamily: 'monospace' }}>
        {diffs.map((line, i) => (
          <div
            key={i}
            style={{
              padding: '0.5rem',
              backgroundColor:
                line.type === 'added' ? '#d1fae5' : line.type === 'removed' ? '#fee2e2' : '#f3f4f6',
              borderLeft: `3px solid ${
                line.type === 'added' ? '#10b981' : line.type === 'removed' ? '#ef4444' : '#d1d5db'
              }`,
            }}
          >
            {line.type === 'added' && '+ '}
            {line.type === 'removed' && '- '}
            {line.type === 'modified' && '~ '}
            {line.currentLine || line.goldenLine}
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Example 3: Diff Statistics Display
 *
 * Use case: Change summary, metrics dashboard, progress indication
 */
export function DiffStatsExample() {
  const [golden, setGolden] = useState(`function add(a, b) {
  return a + b;
}`);
  const [current, setCurrent] = useState(`function add(a, b) {
  const result = a + b;
  return result;
}`);

  const stats = useMemo(() => {
    const diffs = lineDiff(golden, current);
    return getDiffStats(diffs);
  }, [golden, current]);

  return (
    <div>
      <h3>Diff Statistics</h3>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '1rem' }}>
        <textarea value={golden} onChange={(e) => setGolden(e.target.value)} />
        <textarea value={current} onChange={(e) => setCurrent(e.target.value)} />
      </div>
      <div style={{ marginTop: '1rem', display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '1rem' }}>
        <div>
          <strong>+Additions</strong>
          <p>{stats.additions}</p>
        </div>
        <div>
          <strong>-Deletions</strong>
          <p>{stats.deletions}</p>
        </div>
        <div>
          <strong>~Modifications</strong>
          <p>{stats.modifications}</p>
        </div>
        <div>
          <strong>Similarity</strong>
          <p>{stats.similarityScore.toFixed(1)}%</p>
        </div>
      </div>
    </div>
  );
}

/**
 * Example 4: Character-Level Inline Diff
 *
 * Use case: Inline editing, character-level highlighting, autocomplete feedback
 */
export function InlineCharDiffExample() {
  const [text1, setText1] = useState('hello');
  const [text2, setText2] = useState('hallo');

  const diffs = useMemo(() => charDiff(text1, text2), [text1, text2]);

  return (
    <div>
      <h3>Character-Level Diff</h3>
      <input value={text1} onChange={(e) => setText1(e.target.value)} />
      <input value={text2} onChange={(e) => setText2(e.target.value)} />

      <div style={{ marginTop: '1rem', fontFamily: 'monospace' }}>
        <div style={{ marginBottom: '0.5rem' }}>
          <strong>Diff result:</strong>
        </div>
        {diffs.map((diff, i) => (
          <span
            key={i}
            style={{
              backgroundColor:
                diff.type === 'added' ? '#bfdbfe' : diff.type === 'removed' ? '#fecaca' : 'transparent',
              padding: '2px 4px',
            }}
          >
            {diff.value}
          </span>
        ))}
      </div>
    </div>
  );
}

/**
 * Example 5: Progressive Diff Rendering
 *
 * Use case: Large diffs, virtualization, progressive rendering
 */
export function ProgressiveDiffExample() {
  const [golden, setGolden] = useState('word '.repeat(1000));
  const [current, setCurrent] = useState('word '.repeat(999) + 'modified');

  const chunks = useMemo(() => {
    const diffs = tokenDiff(golden, current);
    return chunkDiffs(diffs, 100);
  }, [golden, current]);

  const [visibleChunks, setVisibleChunks] = useState(1);

  return (
    <div>
      <h3>Progressive Diff Rendering</h3>
      <div>Total chunks: {chunks.length}</div>
      <div>Visible chunks: {visibleChunks}</div>
      <button onClick={() => setVisibleChunks((n) => Math.min(n + 1, chunks.length))}>Load more</button>

      <div style={{ marginTop: '1rem', maxHeight: '300px', overflow: 'auto', fontFamily: 'monospace' }}>
        {chunks.slice(0, visibleChunks).map((chunk, chunkIdx) =>
          chunk.map((diff, diffIdx) => (
            <span
              key={`${chunkIdx}-${diffIdx}`}
              style={{
                backgroundColor:
                  diff.type === 'added' ? '#d1fae5' : diff.type === 'removed' ? '#fee2e2' : 'transparent',
              }}
            >
              {diff.value}
            </span>
          ))
        )}
      </div>
    </div>
  );
}

/**
 * Example 6: Merged Diff View
 *
 * Use case: Cleaner diff output, reduced visual noise
 */
export function MergedDiffExample() {
  const [text1, setText1] = useState('aaa bbb ccc ddd');
  const [text2, setText2] = useState('aaa xxx yyy ddd');

  const originalDiffs = useMemo(() => charDiff(text1, text2), [text1, text2]);
  const mergedDiffs = useMemo(() => mergeDiffs(originalDiffs), [originalDiffs]);

  return (
    <div>
      <h3>Merged vs Original Diffs</h3>
      <input value={text1} onChange={(e) => setText1(e.target.value)} />
      <input value={text2} onChange={(e) => setText2(e.target.value)} />

      <div style={{ marginTop: '1rem', display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
        <div>
          <strong>Original ({originalDiffs.length} items):</strong>
          <div style={{ fontFamily: 'monospace', fontSize: '0.8em' }}>
            {originalDiffs.map((d) => `${d.type[0]}:${d.value} `).join('')}
          </div>
        </div>
        <div>
          <strong>Merged ({mergedDiffs.length} items):</strong>
          <div style={{ fontFamily: 'monospace', fontSize: '0.8em' }}>
            {mergedDiffs.map((d) => `${d.type[0]}:${d.value} `).join('')}
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Example 7: Diff Stats Dashboard
 *
 * Use case: Performance metrics, change tracking, analytics
 */
export function DiffStatsDashboardExample() {
  const samples = useMemo(() => [
    { name: 'Minimal change', g: 'hello world', c: 'hello mars' },
    { name: 'Major change', g: 'version 1.0', c: 'version 2.0 with new features' },
    { name: 'Identical', g: 'test', c: 'test' },
    { name: 'Complete rewrite', g: 'old text', c: 'brand new content here' },
  ], []);

  const results = useMemo(
    () =>
      samples.map((s) => ({
        name: s.name,
        score: similarity(s.g, s.c),
        stats: getDiffStats(tokenDiff(s.g, s.c)),
      })),
    [samples]
  );

  return (
    <div>
      <h3>Diff Statistics Dashboard</h3>
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr>
            <th style={{ border: '1px solid #ccc', padding: '0.5rem' }}>Sample</th>
            <th style={{ border: '1px solid #ccc', padding: '0.5rem' }}>Similarity</th>
            <th style={{ border: '1px solid #ccc', padding: '0.5rem' }}>+Adds</th>
            <th style={{ border: '1px solid #ccc', padding: '0.5rem' }}>-Dels</th>
            <th style={{ border: '1px solid #ccc', padding: '0.5rem' }}>~Mods</th>
          </tr>
        </thead>
        <tbody>
          {results.map((r) => (
            <tr key={r.name}>
              <td style={{ border: '1px solid #ccc', padding: '0.5rem' }}>{r.name}</td>
              <td style={{ border: '1px solid #ccc', padding: '0.5rem' }}>{r.score.toFixed(1)}%</td>
              <td style={{ border: '1px solid #ccc', padding: '0.5rem' }}>{r.stats.additions}</td>
              <td style={{ border: '1px solid #ccc', padding: '0.5rem' }}>{r.stats.deletions}</td>
              <td style={{ border: '1px solid #ccc', padding: '0.5rem' }}>{r.stats.modifications}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/**
 * Example 8: Real-world Code Review Diff
 *
 * Use case: Code review systems, version control UI
 */
export function CodeReviewDiffExample() {
  const goldenCode = `function processData(input) {
  const parsed = JSON.parse(input);
  const filtered = parsed.filter(x => x.value > 10);
  return filtered;
}`;

  const currentCode = `function processData(input) {
  const parsed = JSON.parse(input);
  const filtered = parsed.filter(x => x.value > 5);
  const mapped = filtered.map(x => ({ id: x.id, value: x.value * 2 }));
  return mapped;
}`;

  const diffs = useMemo(() => lineDiff(goldenCode, currentCode, true), [goldenCode, currentCode]);
  const stats = useMemo(() => getDiffStats(diffs), [diffs]);

  return (
    <div>
      <h3>Code Review Diff</h3>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
        <div>
          <h4>Before</h4>
          <pre style={{ backgroundColor: '#f3f4f6', padding: '1rem', borderRadius: '4px' }}>{goldenCode}</pre>
        </div>
        <div>
          <h4>After</h4>
          <pre style={{ backgroundColor: '#f3f4f6', padding: '1rem', borderRadius: '4px' }}>{currentCode}</pre>
        </div>
      </div>

      <div style={{ marginTop: '1rem', display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '1rem' }}>
        <div>
          <strong>+{stats.additions}</strong> additions
        </div>
        <div>
          <strong>-{stats.deletions}</strong> deletions
        </div>
        <div>
          <strong>{stats.similarityScore.toFixed(1)}%</strong> similar
        </div>
      </div>

      <div style={{ marginTop: '1rem' }}>
        <h4>Changes:</h4>
        {diffs.map((line, i) => (
          <div key={i} style={{ fontSize: '0.9em', fontFamily: 'monospace' }}>
            {line.type === 'added' && <span style={{ color: 'green' }}>+ </span>}
            {line.type === 'removed' && <span style={{ color: 'red' }}>- </span>}
            {line.type === 'modified' && <span style={{ color: 'orange' }}>~ </span>}
            <span>Line {line.lineNumber}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Export all examples for documentation/testing
 */
export const DiffExamples = {
  SimilarityCheck: SimilarityCheckExample,
  LineDiffDisplay: LineDiffDisplayExample,
  DiffStats: DiffStatsExample,
  InlineCharDiff: InlineCharDiffExample,
  ProgressiveDiff: ProgressiveDiffExample,
  MergedDiff: MergedDiffExample,
  Dashboard: DiffStatsDashboardExample,
  CodeReview: CodeReviewDiffExample,
};

export default DiffExamples;
