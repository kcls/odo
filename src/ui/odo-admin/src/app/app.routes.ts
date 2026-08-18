import { Routes } from '@angular/router';

import { ADMIN_TOOLS } from './core/admin-tools';
import { authGuard, permGuard } from './core/guards';

export const routes: Routes = [
  {
    path: 'login',
    loadComponent: () => import('./features/login/login').then((m) => m.Login),
  },
  {
    path: '',
    loadComponent: () => import('./shell/shell').then((m) => m.Shell),
    canActivate: [authGuard],
    children: [
      {
        path: '',
        pathMatch: 'full',
        loadComponent: () => import('./features/home/home').then((m) => m.Home),
      },
      ...ADMIN_TOOLS.map((tool) => ({
        path: tool.path,
        canActivate: [permGuard(tool.readPerm)],
        loadChildren: tool.loadChildren,
      })),
      { path: '**', redirectTo: '' },
    ],
  },
];
