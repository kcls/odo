import { Component, computed, effect, inject, input, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { MatSelectModule } from '@angular/material/select';
import { MatSnackBar } from '@angular/material/snack-bar';
import { firstValueFrom } from 'rxjs';

import { ApiRequestError } from '../../core/api';
import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import {
  authzAdminApi,
  type GrantRow,
  type PermissionRow,
  type RoleRow,
} from './authz-api';

/**
 * Drill-down page for one role. Everything about the role — its editable
 * details and the permissions it grants — lives on one page as stacked
 * sections, so the page reads as "everything about this role". The permission
 * grants are a bounded nested list rendered in an <odo-table>.
 */
@Component({
  selector: 'app-role-detail',
  imports: [
    FormsModule,
    CdkTableModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    MatSelectModule,
    DetailPage,
    OdoTable,
  ],
  templateUrl: './role-detail.html',
  styleUrl: './role-detail.scss',
})
export class RoleDetailPage {
  /** Route param, bound via withComponentInputBinding. */
  readonly code = input.required<string>();

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly grantColumns = ['perm', 'description', 'min_depth', 'actions'];

  protected readonly loading = signal(true);
  protected readonly role = signal<RoleRow | null>(null);
  protected readonly grants = signal<GrantRow[]>([]);
  protected readonly allPermissions = signal<PermissionRow[]>([]);
  protected readonly canWrite = signal(false);

  // Role edit form state
  protected label = '';
  protected description = '';
  protected readonly saving = signal(false);

  // Add-grant form state
  protected newPerm = '';
  protected newMinDepth = 0;
  protected readonly addingGrant = signal(false);
  protected readonly grantError = signal('');

  /** Summary line under the title: number of users holding the role. */
  protected readonly userCountLabel = computed(() => {
    const role = this.role();
    return role ? $localize`${role.user_count} user(s)` : '';
  });

  /** Permissions not yet granted to this role, for the add-grant select. */
  protected readonly availablePermissions = computed(() => {
    const granted = new Set(this.grants().map((g) => g.perm));
    return this.allPermissions().filter((p) => !granted.has(p.code));
  });

  constructor() {
    this.auth.hasPerm(PERMS.ROLE_WRITE).then((ok) => this.canWrite.set(ok));

    // The permission picker is role-independent: load once.
    void this.loadPermissions();

    // Reload the role whenever the route code changes. This component is reused
    // across same-route :code changes, so ngOnInit would only fire once — an
    // effect tracking code() reloads on every change, including the initial one.
    effect(() => {
      const code = this.code();
      void this.reload(code);
    });
  }

  protected async reload(code: string = this.code()): Promise<void> {
    this.loading.set(true);
    try {
      const detail = await authzAdminApi.getRole(code);
      this.role.set(detail.role);
      this.grants.set(detail.grants);
      this.label = detail.role.label;
      this.description = detail.role.description ?? '';
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 404) {
        this.snackBar.open(
          $localize`Role not found.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
        void this.router.navigate(['/roles']);
        return;
      }
      this.errors.show(err, 'Failed to load role');
    } finally {
      this.loading.set(false);
    }
  }

  private async loadPermissions(): Promise<void> {
    try {
      this.allPermissions.set(await authzAdminApi.listPermissions());
    } catch (err) {
      console.error('Failed to load permissions:', err);
    }
  }

  protected roleDirty(): boolean {
    const role = this.role();
    return (
      !!role &&
      (this.label.trim() !== role.label ||
        this.description.trim() !== (role.description ?? ''))
    );
  }

  protected async saveRole(): Promise<void> {
    const role = this.role();
    if (!role) return;

    this.saving.set(true);
    try {
      const updated = await authzAdminApi.updateRole({
        code: role.code,
        label: this.label.trim(),
        description: this.description.trim(),
      });
      this.role.set(updated);
      this.label = updated.label;
      this.description = updated.description ?? '';
      this.snackBar.open(
        $localize`Role saved.`,
        $localize`:Snackbar dismiss action:Dismiss`,
        { duration: 3000 },
      );
    } catch (err) {
      this.errors.show(err, 'Failed to save role');
    } finally {
      this.saving.set(false);
    }
  }

  protected async deleteRole(): Promise<void> {
    const role = this.role();
    if (!role) return;

    const data: ConfirmDialogData = {
      title: $localize`Delete role?`,
      message: $localize`"${role.label}" and its permission grants will be permanently deleted.`,
      confirmLabel: $localize`Delete`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    const confirmed = await firstValueFrom(ref.afterClosed());
    if (confirmed !== true) return;

    try {
      await authzAdminApi.deleteRole(role.code);
      void this.router.navigate(['/roles']);
    } catch (err) {
      this.errors.show(err, 'Failed to delete role');
    }
  }

  protected async addGrant(): Promise<void> {
    const role = this.role();
    if (!role || !this.newPerm) return;

    this.addingGrant.set(true);
    this.grantError.set('');
    try {
      await authzAdminApi.createGrant({
        role: role.code,
        perm: this.newPerm,
        min_depth: this.newMinDepth,
      });
      this.newPerm = '';
      this.newMinDepth = 0;
      await this.reload();
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'PERMISSION_ALREADY_GRANTED') {
        this.grantError.set($localize`This role already has that permission.`);
      } else {
        this.grantError.set(
          err instanceof Error ? err.message : $localize`Failed to add the grant.`,
        );
      }
    } finally {
      this.addingGrant.set(false);
    }
  }

  protected async updateMinDepth(grant: GrantRow, value: string): Promise<void> {
    const minDepth = Number(value);
    if (!Number.isInteger(minDepth) || minDepth < 0 || minDepth === grant.min_depth) {
      await this.reload();
      return;
    }

    try {
      await authzAdminApi.updateGrant({ id: grant.id, min_depth: minDepth });
      await this.reload();
    } catch (err) {
      await this.reload();
      this.errors.show(err, 'Failed to update grant');
    }
  }

  protected async revokeGrant(grant: GrantRow): Promise<void> {
    const data: ConfirmDialogData = {
      title: $localize`Revoke permission?`,
      message: $localize`${grant.perm} will be revoked from this role.`,
      confirmLabel: $localize`Revoke`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    const confirmed = await firstValueFrom(ref.afterClosed());
    if (confirmed !== true) return;

    try {
      await authzAdminApi.deleteGrant(grant.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to revoke grant');
    }
  }
}
