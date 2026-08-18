import { Routes } from '@angular/router';

export const TEMPLATE_ROUTES: Routes = [
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./template-list').then((m) => m.TemplateList),
  },
  {
    path: 'new',
    loadComponent: () =>
      import('./template-editor').then((m) => m.TemplateEditor),
  },
  {
    path: ':id',
    loadComponent: () =>
      import('./template-editor').then((m) => m.TemplateEditor),
  },
];
