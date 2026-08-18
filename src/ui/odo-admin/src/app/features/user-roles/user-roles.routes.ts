import { Routes } from '@angular/router';

export const USER_ROLE_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./user-search').then((m) => m.UserSearch),
  },
  {
    path: ':id',
    loadComponent: () =>
      import('./user-role-detail').then((m) => m.UserRoleDetail),
  },
];
