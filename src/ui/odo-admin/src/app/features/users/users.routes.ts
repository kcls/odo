import { Routes } from '@angular/router';

export const USER_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./users-search').then((m) => m.UsersSearch),
  },
  {
    path: ':id',
    loadComponent: () => import('./user-detail').then((m) => m.UserDetailPage),
  },
];
