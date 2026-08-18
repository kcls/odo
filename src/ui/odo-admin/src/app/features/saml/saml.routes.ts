import { Routes } from '@angular/router';

export const SAML_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./idp-list').then((m) => m.IdpList),
  },
  {
    path: ':id',
    loadComponent: () => import('./idp-detail').then((m) => m.IdpDetail),
  },
];
