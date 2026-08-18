import { Routes } from '@angular/router';

export const ORG_UNIT_TYPE_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () =>
      import('./org-unit-types').then((m) => m.OrgUnitTypes),
  },
];
