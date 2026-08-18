import { Injectable, effect, signal } from '@angular/core';

export type ThemeMode = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'odo-admin.theme';

/**
 * Light/dark theming. All Material color tokens are emitted as
 * `light-dark()` values (see styles.scss), so switching themes only
 * requires changing `color-scheme` on the document element:
 * 'light dark' defers to the OS preference.
 */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  readonly mode = signal<ThemeMode>(loadSavedMode());

  constructor() {
    effect(() => {
      const mode = this.mode();
      localStorage.setItem(STORAGE_KEY, mode);
      document.documentElement.style.colorScheme =
        mode === 'system' ? 'light dark' : mode;
    });
  }

  setMode(mode: ThemeMode): void {
    this.mode.set(mode);
  }
}

function loadSavedMode(): ThemeMode {
  const saved = localStorage.getItem(STORAGE_KEY);
  return saved === 'light' || saved === 'dark' ? saved : 'system';
}
