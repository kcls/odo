import { Component, OnInit, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { firstValueFrom } from 'rxjs';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { OdoTable, type OdoSort } from '../../shared/odo-table/odo-table';
import { OdoSortHeader } from '../../shared/odo-table/odo-sort-header';
import {
  OdoPaginator,
  type OdoPageEvent,
} from '../../shared/odo-paginator/odo-paginator';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import { authzAdminApi, type PermissionRow } from '../roles/authz-api';
import { PermissionDialog, type PermissionDialogData } from './permission-dialog';

/**
 * Permission list. Permissions are flat reference data — the codes that roles
 * grant. This is the reference **server-driven** list: search, sort, and
 * pagination all go to the API (the table stays presentational). Roles,
 * permissions, and grants are all gated on the one `odo.auth.role.*` perm pair.
 */
@Component({
  selector: 'app-permission-list',
  imports: [
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
  templateUrl: './permission-list.html',
})
export class PermissionList implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['code', 'description', 'roles', 'actions'];

  protected readonly loading = signal(true);
  protected readonly permissions = signal<PermissionRow[]>([]);
  protected readonly total = signal(0);
  protected readonly canWrite = signal(false);

  // Server-driven list state.
  protected readonly search = signal('');
  protected readonly sort = signal<OdoSort>({ active: 'code', direction: 'asc' });
  protected readonly pageIndex = signal(0);
  protected readonly pageSize = signal(25);

  private searchDebounce?: ReturnType<typeof setTimeout>;

  constructor() {
    this.auth.hasPerm(PERMS.ROLE_WRITE).then((ok) => this.canWrite.set(ok));
  }

  ngOnInit(): void {
    void this.reload();
  }

  protected async reload(): Promise<void> {
    this.loading.set(true);
    try {
      const page = await authzAdminApi.listPermissionsPage({
        search: this.search().trim() || undefined,
        sort_by: this.sort().active,
        sort_dir: this.sort().direction,
        limit: this.pageSize(),
        offset: this.pageIndex() * this.pageSize(),
      });
      this.permissions.set(page.rows);
      this.total.set(page.total);
    } catch (err) {
      this.errors.show(err, 'Failed to load permissions');
    } finally {
      this.loading.set(false);
    }
  }

  /** Debounced search; resets to the first page. */
  protected onSearch(value: string): void {
    this.search.set(value);
    clearTimeout(this.searchDebounce);
    this.searchDebounce = setTimeout(() => {
      this.pageIndex.set(0);
      void this.reload();
    }, 300);
  }

  /** Header click: apply the new sort and reload from the first page. */
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

  protected openDialog(permission?: PermissionRow): void {
    const data: PermissionDialogData = { permission };
    const ref = this.dialog.open(PermissionDialog, { data, width: '480px' });
    ref.afterClosed().subscribe((saved?: PermissionRow) => {
      if (saved) void this.reload();
    });
  }

  protected async deletePermission(permission: PermissionRow): Promise<void> {
    const data: ConfirmDialogData = {
      title: $localize`Delete permission?`,
      message: $localize`${permission.code} will be permanently deleted.`,
      confirmLabel: $localize`Delete`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    const confirmed = await firstValueFrom(ref.afterClosed());
    if (confirmed !== true) return;

    try {
      await authzAdminApi.deletePermission(permission.code);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete permission');
    }
  }
}
