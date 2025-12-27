export enum UiMode {
  User = 'user',
  Builder = 'builder',
  Kernel = 'kernel',
  Audit = 'audit',
}

export const UI_MODE_STORAGE_KEY = 'aos_ui_mode';

export const UI_MODE_OPTIONS: UiMode[] = [UiMode.User, UiMode.Builder, UiMode.Kernel, UiMode.Audit];
