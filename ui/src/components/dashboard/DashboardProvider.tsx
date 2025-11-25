import React, { createContext, useContext, useState, useCallback } from 'react';

interface DashboardContextValue {
  refreshInterval: number;
  setRefreshInterval: (interval: number) => void;
  isRefreshing: boolean;
  triggerRefresh: () => void;
  lastRefresh: Date | null;
}

const DashboardContext = createContext<DashboardContextValue | undefined>(undefined);

export function useDashboard() {
  const context = useContext(DashboardContext);
  if (!context) {
    throw new Error('useDashboard must be used within DashboardProvider');
  }
  return context;
}

interface DashboardProviderProps {
  children: React.ReactNode;
}

export function DashboardProvider({ children }: DashboardProviderProps) {
  const [refreshInterval, setRefreshInterval] = useState(30000); // 30 seconds default
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [lastRefresh, setLastRefresh] = useState<Date | null>(null);

  const triggerRefresh = useCallback(() => {
    setIsRefreshing(true);
    setLastRefresh(new Date());

    // Reset refreshing state after a short delay
    setTimeout(() => {
      setIsRefreshing(false);
    }, 500);
  }, []);

  const value: DashboardContextValue = {
    refreshInterval,
    setRefreshInterval,
    isRefreshing,
    triggerRefresh,
    lastRefresh,
  };

  return (
    <DashboardContext.Provider value={value}>
      {children}
    </DashboardContext.Provider>
  );
}
