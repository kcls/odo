import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import { provideRouter, withComponentInputBinding } from '@angular/router';

import { routes } from './app.routes';
import { AuthService } from './core/auth.service';
import { ThemeService } from './core/theme.service';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    // Restore any existing session (HttpOnly refresh cookie) before the
    // router runs so guards see the final auth state, and apply the saved
    // theme before first paint.
    provideAppInitializer(() => {
      inject(ThemeService);
      return inject(AuthService).init();
    }),
  ],
};
