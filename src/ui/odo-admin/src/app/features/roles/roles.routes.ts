import { Routes } from '@angular/router';

export const ROLE_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./role-list').then((m) => m.RoleList),
  },
  {
    path: ':code',
    loadComponent: () => import('./role-detail').then((m) => m.RoleDetailPage),
  },
];
