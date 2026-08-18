import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';

import { AuthService } from './auth.service';

export const authGuard: CanActivateFn = (_route, state) => {
  const auth = inject(AuthService);
  const router = inject(Router);

  if (auth.isAuthenticated()) return true;
  return router.createUrlTree(['/login'], {
    queryParams: { redirect_to: state.url },
  });
};

/**
 * Route gate on a permission. Unauthenticated users go to /login;
 * authenticated users without the perm land on the home page.
 */
export function permGuard(perm: string): CanActivateFn {
  return async (_route, state) => {
    const auth = inject(AuthService);
    const router = inject(Router);

    if (!auth.isAuthenticated()) {
      return router.createUrlTree(['/login'], {
        queryParams: { redirect_to: state.url },
      });
    }
    return (await auth.hasPerm(perm)) ? true : router.createUrlTree(['/']);
  };
}
