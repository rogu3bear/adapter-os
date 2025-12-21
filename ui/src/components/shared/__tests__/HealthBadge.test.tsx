import React from 'react';
import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { AdapterHealthFlag } from '@/api/adapter-types';
import { HealthBadge } from '@/components/shared/TrustHealthBadge';
import { TooltipProvider } from '@/components/ui/tooltip';

describe('HealthBadge snapshots', () => {
  const states: AdapterHealthFlag[] = ['healthy', 'degraded', 'unsafe', 'corrupt'];

  states.forEach(state => {
    it(`renders ${state} state`, () => {
      const { container, getByText } = render(
        <TooltipProvider>
          <HealthBadge state={state} />
        </TooltipProvider>
      );
      expect(getByText(/Healthy|Degraded|Unsafe|Corrupt/)).toBeInTheDocument();
      const text = container.textContent;
      switch (state) {
        case 'healthy':
          expect(text).toMatchInlineSnapshot('"Healthy"');
          break;
        case 'degraded':
          expect(text).toMatchInlineSnapshot('"Degraded"');
          break;
        case 'unsafe':
          expect(text).toMatchInlineSnapshot('"Unsafe"');
          break;
        case 'corrupt':
          expect(text).toMatchInlineSnapshot('"Corrupt"');
          break;
      }
    });
  });
});
