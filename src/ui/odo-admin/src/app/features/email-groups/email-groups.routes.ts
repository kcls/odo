import { Routes } from '@angular/router';

export const EMAIL_GROUP_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () =>
      import('./email-group-list').then((m) => m.EmailGroupList),
  },
  {
    path: ':id',
    loadComponent: () =>
      import('./email-group-detail').then((m) => m.EmailGroupDetail),
  },
];
