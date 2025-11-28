// 【ui/src/hooks/useInformationDensity.ts§1-122】 - Information density hook pattern
// 【ui/src/components/Dashboard.tsx§37-38】 - Dashboard usage pattern
import { createContext, useContext, ReactNode } from 'react';
import { useInformationDensity, InformationDensity, InformationDensityConfig } from '@/hooks/useInformationDensity';

interface DensityContextValue {
  density: InformationDensity;
  setDensity: (density: InformationDensity) => void;
  spacing: ReturnType<typeof useInformationDensity>['spacing'];
  textSizes: ReturnType<typeof useInformationDensity>['textSizes'];
  isCompact: boolean;
  isComfortable: boolean;
  isSpacious: boolean;
}

const DensityContext = createContext<DensityContextValue | undefined>(undefined);

interface DensityProviderProps {
  children: ReactNode;
  pageKey: string;
  defaultDensity?: InformationDensity;
  persist?: boolean;
}

export function DensityProvider({ 
  children, 
  pageKey, 
  defaultDensity = 'comfortable',
  persist = true 
}: DensityProviderProps) {
  const config: InformationDensityConfig = {
    key: `page-${pageKey}`,
    defaultDensity,
    persist
  };
  
  const densityHook = useInformationDensity(config);

  const value: DensityContextValue = {
    density: densityHook.density,
    setDensity: densityHook.setDensity,
    spacing: densityHook.spacing,
    textSizes: densityHook.textSizes,
    isCompact: densityHook.isCompact,
    isComfortable: densityHook.isComfortable,
    isSpacious: densityHook.isSpacious
  };

  return (
    <DensityContext.Provider value={value}>
      {children}
    </DensityContext.Provider>
  );
}

export function useDensity(): DensityContextValue {
  const context = useContext(DensityContext);
  if (!context) {
    throw new Error('useDensity must be used within DensityProvider');
  }
  return context;
}

