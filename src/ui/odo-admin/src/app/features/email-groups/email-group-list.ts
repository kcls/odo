import { Component, OnInit, inject, signal } from '@angular/core';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { OdoTable, type OdoSort } from '../../shared/odo-table/odo-table';
import { OdoSortHeader } from '../../shared/odo-table/odo-sort-header';
import {
  OdoPaginator,
  type OdoPageEvent,
} from '../../shared/odo-paginator/odo-paginator';
import { emailGroupApi, type EmailGroupRow } from './email-groups-api';
import { CreateEmailGroupDialog } from './create-email-group-dialog';

/**
 * Email group list. Each group is a subject the user drills into, so the list
 * is a clickable framed table. Server-driven: search, sort, pagination, and the
 * include-inactive toggle all go to the API.
 */
@Component({
  selector: 'app-email-group-list',
  imports: [
    DatePipe,
    FormsModule,
    CdkTableModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    MatSlideToggleModule,
    OdoTable,
    OdoSortHeader,
    OdoPaginator,
  ],
  templateUrl: './email-group-list.html',
  styleUrl: './email-group-list.scss',
})
export class EmailGroupList implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['code', 'label', 'status', 'members', 'updated'];

  protected readonly loading = signal(true);
  protected readonly groups = signal<EmailGroupRow[]>([]);
  protected readonly total = signal(0);
  protected readonly includeInactive = signal(false);
  protected readonly canWrite = signal(false);

  protected readonly search = signal('');
  protected readonly sort = signal<OdoSort>({ active: 'code', direction: 'asc' });
  protected readonly pageIndex = signal(0);
  protected readonly pageSize = signal(25);

  private searchDebounce?: ReturnType<typeof setTimeout>;

  constructor() {
    this.auth.hasPerm(PERMS.EMAIL_GROUP_WRITE).then((ok) => {
      this.canWrite.set(ok);
    });
  }

  ngOnInit(): void {
    void this.reload();
  }

  protected async reload(): Promise<void> {
    this.loading.set(true);
    try {
      const page = await emailGroupApi.listPage({
        search: this.search().trim() || undefined,
        include_inactive: this.includeInactive(),
        sort_by: this.sort().active,
        sort_dir: this.sort().direction,
        limit: this.pageSize(),
        offset: this.pageIndex() * this.pageSize(),
      });
      this.groups.set(page.rows);
      this.total.set(page.total);
    } catch (err) {
      this.errors.show(err, 'Failed to load email groups');
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

  protected toggleInactive(checked: boolean): void {
    this.includeInactive.set(checked);
    this.pageIndex.set(0);
    void this.reload();
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

  protected open(group: EmailGroupRow): void {
    void this.router.navigate(['/email-groups', group.id]);
  }

  protected createGroup(): void {
    const ref = this.dialog.open(CreateEmailGroupDialog, { width: '420px' });
    ref.afterClosed().subscribe((created?: EmailGroupRow) => {
      if (created) {
        void this.router.navigate(['/email-groups', created.id]);
      }
    });
  }
}
