import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { vi } from 'vitest';
import { LineageViewer } from '../LineageViewer';
import type { LineageGraphResponse } from '@/api/types';

const mockData: LineageGraphResponse = {
  root: { id: 'ds-v1', type: 'dataset_version', label: 'Dataset v1' },
  upstream: [
    {
      type: 'document',
      label: 'Documents',
      nodes: [
        { id: 'doc-1', type: 'document', label: 'Doc 1' },
        { id: 'doc-2', type: 'document', label: 'Doc 2' },
      ],
    },
  ],
  downstream: [
    {
      type: 'training_job',
      label: 'Jobs',
      nodes: [{ id: 'job-1', type: 'training_job', label: 'Job 1' }],
      total: 2,
      has_more: true,
      next_cursor: 'cursor-1',
    },
  ],
};

describe('LineageViewer', () => {
  it('renders upstream and downstream nodes', () => {
    render(
      <LineageViewer
        title="Test Lineage"
        data={mockData}
        isLoading={false}
        direction="both"
        includeEvidence={false}
        onChangeDirection={() => {}}
        onToggleEvidence={() => {}}
        onNavigateNode={() => {}}
      />,
    );

    expect(screen.getByText('Test Lineage')).toBeInTheDocument();
    expect(screen.getByText('Upstream')).toBeInTheDocument();
    expect(screen.getByText('Downstream')).toBeInTheDocument();
    expect(screen.getByText('Doc 1')).toBeInTheDocument();
    expect(screen.getByText('Job 1')).toBeInTheDocument();
  });

  it('invokes load more when provided', () => {
    const onLoadMore = vi.fn();
    render(
      <LineageViewer
        title="Test Lineage"
        data={mockData}
        isLoading={false}
        direction="both"
        includeEvidence={false}
        onChangeDirection={() => {}}
        onToggleEvidence={() => {}}
        onNavigateNode={() => {}}
        onLoadMore={onLoadMore}
      />,
    );

    const button = screen.getByText('See more');
    fireEvent.click(button);
    expect(onLoadMore).toHaveBeenCalled();
  });
});
