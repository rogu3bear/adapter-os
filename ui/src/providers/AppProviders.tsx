import React, { ReactNode } from 'react';
import { CoreProviders } from './CoreProviders';
import { FeatureProviders } from './FeatureProviders';

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <CoreProviders>
      <FeatureProviders>
        {children}
      </FeatureProviders>
    </CoreProviders>
  );
}

