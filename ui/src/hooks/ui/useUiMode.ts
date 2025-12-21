import { useCallback, useState } from 'react';
import { UiMode, UI_MODE_OPTIONS, UI_MODE_STORAGE_KEY } from '@/config/ui-mode';
import { readLocalStorage, writeLocalStorage } from '@/utils/storage';

const parseMode = (value: string | null): UiMode => {
  if (!value) return UiMode.User;
  const match = UI_MODE_OPTIONS.find(mode => mode === value);
  return match ?? UiMode.User;
};

export function useUiMode() {
  const [uiMode, setUiModeState] = useState<UiMode>(() => {
    const saved = readLocalStorage(UI_MODE_STORAGE_KEY);
    return parseMode(saved);
  });

  const setUiMode = useCallback((mode: UiMode) => {
    setUiModeState(mode);
    writeLocalStorage(UI_MODE_STORAGE_KEY, mode);
  }, []);

  return {
    uiMode,
    setUiMode,
    availableModes: UI_MODE_OPTIONS,
  };
}
