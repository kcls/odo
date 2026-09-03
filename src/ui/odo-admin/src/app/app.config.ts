import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from '@angular/core';
import {
  PreloadAllModules,
  provideRouter,
  withComponentInputBinding,
  withPreloading,
} from '@angular/router';

import { routes } from './app.routes';
import { AuthService } from './core/auth.service';
import { ThemeService } from './core/theme.service';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    // Lazy route chunks are fetched in the background once the initial
    // bundle has run, so only the first navigation pays a round trip. The
    // whole app is ~1.2MB of JS; on a high-latency link the per-navigation
    // waterfall costs far more than downloading it up front.
    provideRouter(routes, withComponentInputBinding(), withPreloading(PreloadAllModules)),
    // Restore any existing session (HttpOnly refresh cookie) before the
    // router runs so guards see the final auth state, and apply the saved
    // theme before first paint.
    provideAppInitializer(() => {
      inject(ThemeService);
      return inject(AuthService).init();
    }),
  ],
};
