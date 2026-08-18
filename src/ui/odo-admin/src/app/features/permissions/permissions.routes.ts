import { Routes } from '@angular/router';

export const PERMISSION_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () =>
      import('./permission-list').then((m) => m.PermissionList),
  },
];
