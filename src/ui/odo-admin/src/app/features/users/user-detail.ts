import {
  Component,
  computed,
  effect,
  inject,
  input,
  numberAttribute,
  signal,
} from '@angular/core';
import { DatePipe } from '@angular/common';
import { Router, RouterLink } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
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
import { userAdminApi, type UserAccount, type UserDetail } from './user-api';
import { EditUserDialog, type EditUserDialogData } from './edit-user-dialog';

/**
 * Drill-down page for one user. Rather than tabbing between disconnected lists,
 * every facet of the account — its metadata, role assignments, SAML identity,
 * and recent sessions — is a stacked section on one page, so the page reads as
 * "everything about this user".
 */
@Component({
  selector: 'app-user-detail',
  imports: [
    DatePipe,
    RouterLink,
    CdkTableModule,
    MatButtonModule,
    MatIconModule,
    MatProgressSpinnerModule,
    DetailPage,
    OdoTable,
  ],
  templateUrl: './user-detail.html',
  styleUrl: './user-detail.scss',
})
export class UserDetailPage {
  /** Route param, bound via withComponentInputBinding. */
  readonly id = input.required({ transform: numberAttribute });

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly sessionColumns = [
    'created',
    'last_activity',
    'expires',
    'method',
    'org_unit',
    'ip',
    'agent',
    'status',
  ];

  protected readonly loading = signal(true);
  protected readonly detail = signal<UserDetail | null>(null);
  protected readonly canManageRoles = signal(false);
  protected readonly canWrite = signal(false);

  /** Only local accounts are editable; SAML accounts are IdP-owned. */
  protected readonly editable = computed(
    () => this.canWrite() && this.detail()?.user.auth_method === 'local',
  );

  constructor() {
    this.auth.hasPerm(PERMS.USER_ROLE_WRITE).then((ok) => this.canManageRoles.set(ok));
    this.auth.hasPerm(PERMS.USER_WRITE).then((ok) => this.canWrite.set(ok));

    // Reload whenever the route id changes. This component is reused across
    // same-route :id changes, so ngOnInit would only fire once — an effect
    // tracking id() reloads on every change, including the initial one.
    effect(() => {
      const id = this.id();
      void this.load(id);
    });
  }

  private async load(id: number = this.id()): Promise<void> {
    this.loading.set(true);
    try {
      this.detail.set(await userAdminApi.getDetail(id));
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 404) {
        this.snackBar.open(
          $localize`User not found.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
        void this.router.navigate(['/users']);
        return;
      }
      this.errors.show(err, 'Failed to load user detail');
    } finally {
      this.loading.set(false);
    }
  }

  private patchUser(user: UserAccount): void {
    const current = this.detail();
    if (current) this.detail.set({ ...current, user });
  }

  protected edit(): void {
    const user = this.detail()?.user;
    if (!user) return;
    const data: EditUserDialogData = { user };
    const ref = this.dialog.open(EditUserDialog, { data, width: '440px' });
    ref.afterClosed().subscribe((updated?: UserAccount) => {
      if (updated) this.patchUser(updated);
    });
  }

  protected async setDeleted(deleted: boolean): Promise<void> {
    const user = this.detail()?.user;
    if (!user) return;

    if (deleted) {
      const data: ConfirmDialogData = {
        title: $localize`Mark account deleted?`,
        message: $localize`${user.display_name} will be marked deleted and blocked from signing in. This can be undone.`,
        confirmLabel: $localize`Mark deleted`,
      };
      const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
      const confirmed = await firstValueFrom(ref.afterClosed());
      if (confirmed !== true) return;
    }

    try {
      this.patchUser(await userAdminApi.updateUser(user.id, { deleted }));
    } catch (err) {
      this.errors.show(err, 'Failed to update deletion state');
    }
  }
}
