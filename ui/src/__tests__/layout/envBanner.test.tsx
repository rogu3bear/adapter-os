import { render, screen } from '@testing-library/react';
import { describe, expect, test } from 'vitest';

import { SessionModeBanner } from '@/layout/RootLayout';

describe('SessionModeBanner', () => {
  test('renders API base URL in dev', () => {
    render(<SessionModeBanner sessionMode="normal" />);

    expect(screen.getByTestId('env-banner')).toBeInTheDocument();
    expect(screen.getByText('API:')).toBeInTheDocument();
    expect(screen.getByText('/api')).toBeInTheDocument();
    expect(screen.queryByText(/Commit:/)).not.toBeInTheDocument();
  });

  test('renders short commit sha when available', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- tests modify import.meta.env
    const meta = import.meta as any;
    Object.assign(meta.env ?? {}, { VITE_COMMIT_SHA: 'abcdef1234567890' });

    render(<SessionModeBanner sessionMode="normal" />);

    expect(screen.getByText('Commit:')).toBeInTheDocument();
    expect(screen.getByText('abcdef12')).toBeInTheDocument();
  });
});
