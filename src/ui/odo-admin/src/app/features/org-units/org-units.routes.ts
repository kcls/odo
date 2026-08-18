import { Routes } from '@angular/router';

export const ORG_UNIT_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./org-units').then((m) => m.OrgUnits),
  },
  {
    path: ':id',
    loadComponent: () => import('./org-unit-detail').then((m) => m.OrgUnitDetail),
  },
];
