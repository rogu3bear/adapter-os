import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { EvidenceStatusBadge } from '@/components/evidence/EvidenceStatusBadge';
import type { EvidenceStatus } from '@/api/document-types';

describe('EvidenceStatusBadge', () => {
  it('renders without crashing', () => {
    render(<EvidenceStatusBadge status="ready" />);
    expect(screen.getByText('Ready')).toBeInTheDocument();
  });

  it('displays "Queued" for queued status', () => {
    render(<EvidenceStatusBadge status="queued" />);
    expect(screen.getByText('Queued')).toBeInTheDocument();
  });

  it('displays "Building" for building status', () => {
    render(<EvidenceStatusBadge status="building" />);
    expect(screen.getByText('Building')).toBeInTheDocument();
  });

  it('displays "Ready" for ready status', () => {
    render(<EvidenceStatusBadge status="ready" />);
    expect(screen.getByText('Ready')).toBeInTheDocument();
  });

  it('displays "Failed" for failed status', () => {
    render(<EvidenceStatusBadge status="failed" />);
    expect(screen.getByText('Failed')).toBeInTheDocument();
  });

  it('displays "Unknown" when status is null', () => {
    render(<EvidenceStatusBadge status={null} />);
    expect(screen.getByText('Unknown')).toBeInTheDocument();
  });

  it('displays "Unknown" when status is undefined', () => {
    render(<EvidenceStatusBadge />);
    expect(screen.getByText('Unknown')).toBeInTheDocument();
  });

  it('applies correct styling for queued status', () => {
    render(<EvidenceStatusBadge status="queued" />);
    const badge = screen.getByText('Queued');
    expect(badge.className).toContain('text-amber-700');
    expect(badge.className).toContain('border-amber-200');
    expect(badge.className).toContain('bg-amber-50');
  });

  it('applies correct styling for building status', () => {
    render(<EvidenceStatusBadge status="building" />);
    const badge = screen.getByText('Building');
    expect(badge.className).toContain('text-blue-700');
    expect(badge.className).toContain('border-blue-200');
    expect(badge.className).toContain('bg-blue-50');
  });

  it('applies correct styling for ready status', () => {
    render(<EvidenceStatusBadge status="ready" />);
    const badge = screen.getByText('Ready');
    expect(badge.className).toContain('text-green-700');
    expect(badge.className).toContain('border-green-200');
    expect(badge.className).toContain('bg-green-50');
  });

  it('applies correct styling for failed status', () => {
    render(<EvidenceStatusBadge status="failed" />);
    const badge = screen.getByText('Failed');
    expect(badge.className).toContain('text-red-700');
    expect(badge.className).toContain('border-red-200');
    expect(badge.className).toContain('bg-red-50');
  });

  it('applies correct styling for unknown status', () => {
    render(<EvidenceStatusBadge status={null} />);
    const badge = screen.getByText('Unknown');
    expect(badge.className).toContain('text-slate-700');
    expect(badge.className).toContain('border-slate-200');
    expect(badge.className).toContain('bg-slate-50');
  });
});
