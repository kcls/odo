import {
  Component,
  effect,
  inject,
  input,
  numberAttribute,
  signal,
} from '@angular/core';
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
import {
  MatSlideToggle,
  MatSlideToggleModule,
} from '@angular/material/slide-toggle';
import { MatSnackBar } from '@angular/material/snack-bar';
import { firstValueFrom } from 'rxjs';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { ApiRequestError } from '../../core/api';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import {
  emailGroupApi,
  type EmailGroupRow,
  type EmailGroupMemberRow,
} from './email-groups-api';

/**
 * Drill-down page for one email group. Everything about the group — its
 * editable code/label, its active state, and its members — lives on one page
 * as stacked sections, so the page reads as "everything about this group".
 */
@Component({
  selector: 'app-email-group-detail',
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
    DetailPage,
    OdoTable,
  ],
  templateUrl: './email-group-detail.html',
  styleUrl: './email-group-detail.scss',
})
export class EmailGroupDetail {
  /** Route param, bound via withComponentInputBinding. */
  readonly id = input.required({ transform: numberAttribute });

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly memberColumns = ['email', 'active', 'added', 'actions'];

  protected readonly loading = signal(true);
  protected readonly group = signal<EmailGroupRow | null>(null);
  protected readonly members = signal<EmailGroupMemberRow[]>([]);
  protected readonly canWrite = signal(false);

  // Group edit form state
  protected code = '';
  protected label = '';
  protected readonly saving = signal(false);
  protected readonly codeError = signal('');

  // Add-member form state
  protected newEmail = '';
  protected readonly addingMember = signal(false);
  protected readonly memberError = signal('');

  constructor() {
    this.auth.hasPerm(PERMS.EMAIL_GROUP_WRITE).then((ok) => {
      this.canWrite.set(ok);
    });

    // Reload whenever the route id changes. This component is reused across
    // same-route :id changes, so ngOnInit would only fire once — an effect
    // tracking id() reloads on every change, including the initial one.
    effect(() => {
      const id = this.id();
      void this.reload(id);
    });
  }

  protected async reload(id: number = this.id()): Promise<void> {
    this.loading.set(true);
    try {
      const detail = await emailGroupApi.get(id);
      this.group.set(detail.group);
      this.members.set(detail.members);
      this.code = detail.group.code;
      this.label = detail.group.label;
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 404) {
        this.snackBar.open(
          $localize`Email group not found.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
        void this.router.navigate(['/email-groups']);
        return;
      }
      this.errors.show(err, 'Failed to load email group');
    } finally {
      this.loading.set(false);
    }
  }

  protected groupDirty(): boolean {
    const group = this.group();
    return (
      !!group && (this.code.trim() !== group.code || this.label.trim() !== group.label)
    );
  }

  protected async saveGroup(): Promise<void> {
    const group = this.group();
    if (!group) return;

    this.saving.set(true);
    this.codeError.set('');
    try {
      const updated = await emailGroupApi.update({
        id: group.id,
        code: this.code.trim(),
        label: this.label.trim(),
      });
      this.group.set(updated);
      this.code = updated.code;
      this.label = updated.label;
      this.snackBar.open(
        $localize`Group saved.`,
        $localize`:Snackbar dismiss action:Dismiss`,
        { duration: 3000 },
      );
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'EMAIL_GROUP_CODE_TAKEN') {
        this.codeError.set($localize`An email group with this code already exists.`);
      } else {
        this.errors.show(err, 'Failed to save group');
      }
    } finally {
      this.saving.set(false);
    }
  }

  protected async toggleGroupActive(
    active: boolean,
    toggle: MatSlideToggle,
  ): Promise<void> {
    const group = this.group();
    if (!group) return;

    if (!active) {
      const confirmed = await this.confirm({
        title: $localize`Deactivate email group?`,
        message: $localize`"${group.label}" will stop receiving notification emails until it is reactivated. Existing configuration is kept.`,
        confirmLabel: $localize`Deactivate`,
      });
      if (!confirmed) {
        // The bound is_active value hasn't changed, so change detection
        // won't reset the widget; flip it back directly.
        toggle.checked = group.is_active;
        return;
      }
    }

    try {
      const updated = await emailGroupApi.update({ id: group.id, is_active: active });
      this.group.set(updated);
    } catch (err) {
      toggle.checked = group.is_active;
      this.errors.show(err, 'Failed to update group status');
    }
  }

  protected async addMember(): Promise<void> {
    const group = this.group();
    const email = this.newEmail.trim();
    if (!group || !email) return;

    this.addingMember.set(true);
    this.memberError.set('');
    try {
      await emailGroupApi.addMember({ email_group: group.id, email });
      this.newEmail = '';
      await this.reload();
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'EMAIL_ALREADY_IN_GROUP') {
        this.memberError.set(
          $localize`This email address is already a member of the group.`,
        );
      } else if (err instanceof ApiRequestError && err.status === 400) {
        this.memberError.set($localize`Enter a valid email address.`);
      } else {
        this.memberError.set(
          err instanceof Error ? err.message : $localize`Failed to add the member.`,
        );
      }
    } finally {
      this.addingMember.set(false);
    }
  }

  protected async toggleMemberActive(
    member: EmailGroupMemberRow,
    active: boolean,
  ): Promise<void> {
    try {
      await emailGroupApi.updateMember({ id: member.id, is_active: active });
      await this.reload();
    } catch (err) {
      await this.reload();
      this.errors.show(err, 'Failed to update member');
    }
  }

  protected async deleteMember(member: EmailGroupMemberRow): Promise<void> {
    const confirmed = await this.confirm({
      title: $localize`Remove member?`,
      message: $localize`${member.email} will be permanently removed from this group.`,
      confirmLabel: $localize`Remove`,
    });
    if (!confirmed) return;

    try {
      await emailGroupApi.deleteMember(member.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to remove member');
    }
  }

  private confirm(data: ConfirmDialogData): Promise<boolean> {
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    return firstValueFrom(ref.afterClosed()).then((result) => result === true);
  }
}
