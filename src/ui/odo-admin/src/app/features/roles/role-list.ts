import { Component, OnInit, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
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
import { authzAdminApi, type RoleRow } from './authz-api';
import { CreateRoleDialog } from './create-role-dialog';

/**
 * Role list. A role is the subject the user drills into (to manage its
 * permission grants), so rows are clickable and open the role detail page.
 * Server-driven: search, sort, and pagination all go to the API.
 */
@Component({
  selector: 'app-role-list',
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
  templateUrl: './role-list.html',
})
export class RoleList implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['code', 'label', 'permissions', 'users'];

  protected readonly loading = signal(true);
  protected readonly roles = signal<RoleRow[]>([]);
  protected readonly total = signal(0);
  protected readonly canWrite = signal(false);

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
      const page = await authzAdminApi.listRolesPage({
        search: this.search().trim() || undefined,
        sort_by: this.sort().active,
        sort_dir: this.sort().direction,
        limit: this.pageSize(),
        offset: this.pageIndex() * this.pageSize(),
      });
      this.roles.set(page.rows);
      this.total.set(page.total);
    } catch (err) {
      this.errors.show(err, 'Failed to load roles');
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

  protected open(role: RoleRow): void {
    void this.router.navigate(['/roles', role.code]);
  }

  protected createRole(): void {
    const ref = this.dialog.open(CreateRoleDialog, { width: '480px' });
    ref.afterClosed().subscribe((created?: RoleRow) => {
      if (created) {
        void this.router.navigate(['/roles', created.code]);
      }
    });
  }
}
