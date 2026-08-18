import { Component, OnInit, inject, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { OdoTable, type OdoSort } from '../../shared/odo-table/odo-table';
import { OdoSortHeader } from '../../shared/odo-table/odo-sort-header';
import {
  OdoPaginator,
  type OdoPageEvent,
} from '../../shared/odo-paginator/odo-paginator';
import { templatesApi, type TemplateRow } from './templates-api';

/**
 * Notification template list. Templates are the subject the user drills into,
 * so the list is a framed table with clickable rows that open the full-page
 * editor. Server-driven: search, sort, and pagination all go to the API.
 */
@Component({
  selector: 'app-template-list',
  imports: [
    DatePipe,
    FormsModule,
    CdkTableModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    OdoTable,
    OdoSortHeader,
    OdoPaginator,
  ],
  templateUrl: './template-list.html',
})
export class TemplateList implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['code', 'name', 'status', 'updated'];

  protected readonly loading = signal(true);
  protected readonly templates = signal<TemplateRow[]>([]);
  protected readonly total = signal(0);
  protected readonly canWrite = signal(false);

  protected readonly search = signal('');
  protected readonly sort = signal<OdoSort>({ active: 'code', direction: 'asc' });
  protected readonly pageIndex = signal(0);
  protected readonly pageSize = signal(25);

  private searchDebounce?: ReturnType<typeof setTimeout>;

  constructor() {
    this.auth.hasPerm(PERMS.TEMPLATE_WRITE).then((ok) => {
      this.canWrite.set(ok);
    });
  }

  ngOnInit(): void {
    void this.reload();
  }

  protected async reload(): Promise<void> {
    this.loading.set(true);
    try {
      const page = await templatesApi.listPage({
        search: this.search().trim() || undefined,
        sort_by: this.sort().active,
        sort_dir: this.sort().direction,
        limit: this.pageSize(),
        offset: this.pageIndex() * this.pageSize(),
      });
      this.templates.set(page.rows);
      this.total.set(page.total);
    } catch (err) {
      this.errors.show(err, 'Failed to load templates');
    } finally {
      this.loading.set(false);
    }
  }

  protected onSearch(value: string): void {
    this.search.set(value);
    clearTimeout(this.searchDebounce);
    this.searchDebounce = setTimeout(() => {
      this.pageIndex.set(0);
      void this.reload();
    }, 300);
  }

  protected onSort(sort: OdoSort): void {
    this.sort.set(sort);
    this.pageIndex.set(0);
    void this.reload();
  }

  protected onPage(event: OdoPageEvent): void {
    this.pageIndex.set(event.pageIndex);
    this.pageSize.set(event.pageSize);
    void this.reload();
  }

  protected open(template: TemplateRow): void {
    void this.router.navigate(['/templates', template.id]);
  }

  protected createNew(): void {
    void this.router.navigate(['/templates/new']);
  }
}
