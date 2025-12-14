import { useState, useEffect } from 'react';

export type InformationDensity = 'compact' | 'comfortable' | 'spacious';

export interface InformationDensityConfig {
  key: string;
  defaultDensity?: InformationDensity;
  persist?: boolean;
}


export interface UseInformationDensityReturn {
  density: InformationDensity;
  setDensity: (density: InformationDensity) => void;
  spacing: {
    cardPadding: string;
    sectionGap: string;
    gridGap: string;
    buttonGap: string;
    formFieldGap: string;
    tableCellPadding: string;
    modalPadding: string;
  };
  textSizes: {
    title: string;
    subtitle: string;
    body: string;
    caption: string;
  };
  isCompact: boolean;
  isComfortable: boolean;
  isSpacious: boolean;
}

/**
 * Hook for managing information density settings with optional localStorage persistence.
 *
 * @param config - Configuration object
 * @param config.key - Unique key for localStorage persistence
 * @param config.defaultDensity - Initial density setting (default: 'comfortable')
 * @param config.persist - Whether to persist state in localStorage (default: true)
 * @returns Object with density state, setter, and utility functions
 */
export function useInformationDensity(config: InformationDensityConfig): UseInformationDensityReturn {
  const { key, defaultDensity = 'comfortable', persist = true } = config;
  
  // Get initial state from localStorage if persistence is enabled
  const getInitialState = (): InformationDensity => {
    if (persist) {
      const saved = localStorage.getItem(`information-density-${key}`);
      return (saved as InformationDensity) || defaultDensity;
    }
    return defaultDensity;
  };

  const [density, setDensity] = useState<InformationDensity>(getInitialState);

  // Persist state changes to localStorage
  useEffect(() => {
    if (persist) {
      localStorage.setItem(`information-density-${key}`, density);
    }
  }, [density, key, persist]);

  // Density-based spacing values
  const getSpacing = () => {
    switch (density) {
      case 'compact':
        return {
          cardPadding: 'p-3',
          sectionGap: 'space-y-3',
          gridGap: 'gap-3',
          buttonGap: 'gap-1',
          formFieldGap: 'space-y-2',
          tableCellPadding: 'px-2 py-1',
          modalPadding: 'p-4'
        };
      case 'comfortable':
        return {
          cardPadding: 'p-4',
          sectionGap: 'space-y-4',
          gridGap: 'gap-4',
          buttonGap: 'gap-2',
          formFieldGap: 'space-y-3',
          tableCellPadding: 'px-3 py-2',
          modalPadding: 'p-6'
        };
      case 'spacious':
        return {
          cardPadding: 'p-6',
          sectionGap: 'space-y-6',
          gridGap: 'gap-6',
          buttonGap: 'gap-3',
          formFieldGap: 'space-y-4',
          tableCellPadding: 'px-4 py-3',
          modalPadding: 'p-8'
        };
      default:
        return {
          cardPadding: 'p-4',
          sectionGap: 'space-y-4',
          gridGap: 'gap-4',
          buttonGap: 'gap-2',
          formFieldGap: 'space-y-3',
          tableCellPadding: 'px-3 py-2',
          modalPadding: 'p-6'
        };
    }
  };

  // Density-based text sizes
  const getTextSizes = () => {
    switch (density) {
      case 'compact':
        return {
          title: 'text-lg',
          subtitle: 'text-sm',
          body: 'text-xs',
          caption: 'text-xs'
        };
      case 'comfortable':
        return {
          title: 'text-xl',
          subtitle: 'text-base',
          body: 'text-sm',
          caption: 'text-xs'
        };
      case 'spacious':
        return {
          title: 'text-2xl',
          subtitle: 'text-lg',
          body: 'text-base',
          caption: 'text-sm'
        };
      default:
        return {
          title: 'text-xl',
          subtitle: 'text-base',
          body: 'text-sm',
          caption: 'text-xs'
        };
    }
  };

  return {
    density,
    setDensity,
    spacing: getSpacing(),
    textSizes: getTextSizes(),
    isCompact: density === 'compact',
    isComfortable: density === 'comfortable',
    isSpacious: density === 'spacious'
  };
}
