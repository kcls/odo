import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';

import { ApiRequestError } from '../../core/api';
import { emailGroupApi, type EmailGroupRow } from './email-groups-api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';

/** Create-group dialog; resolves with the new group on success. */
@Component({
  selector: 'app-create-email-group-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
  ],
  template: `
    <h2 mat-dialog-title i18n="Dialog title">New Email Group</h2>
    <mat-dialog-content>
      <div class="dialog-form">
      <mat-form-field appearance="outline" class="dialog-field">
        <mat-label i18n="Email group field label">Code</mat-label>
        <input
          matInput
          name="code"
          [(ngModel)]="code"
          required
          i18n-placeholder="Example email group code"
          placeholder="e.g. shoreline-staff"
            [errorStateMatcher]="codeMatcher"
        />
        <mat-hint i18n="Help text for the email group code field"
          >Unique identifier; not shown to recipients.</mat-hint
        >
        @if (codeError()) {
          <mat-error>{{ codeError() }}</mat-error>
        }
      </mat-form-field>

      <mat-form-field appearance="outline" class="dialog-field">
        <mat-label i18n="Email group field label">Label</mat-label>
        <input
          matInput
          name="label"
          [(ngModel)]="label"
          required
          i18n-placeholder="Example email group label"
          placeholder="e.g. Shoreline Staff"
        />
      </mat-form-field>

      @if (error()) {
        <p class="dialog-error" role="alert">{{ error() }}</p>
      }
      </div>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button mat-dialog-close i18n="Dialog dismiss button">Cancel</button>
      <button
        mat-flat-button
        [disabled]="saving() || !code.trim() || !label.trim()"
        (click)="save()"
        i18n="Dialog action that creates the email group"
      >
        Create
      </button>
    </mat-dialog-actions>
  `,
})
export class CreateEmailGroupDialog {
  private readonly dialogRef =
    inject<MatDialogRef<CreateEmailGroupDialog, EmailGroupRow>>(MatDialogRef);

  protected code = '';
  protected label = '';
  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly codeError = signal('');
  protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.codeError.set('');
    try {
      const group = await emailGroupApi.create({
        code: this.code.trim(),
        label: this.label.trim(),
      });
      this.dialogRef.close(group);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'EMAIL_GROUP_CODE_TAKEN') {
        this.codeError.set(
          $localize`An email group with this code already exists.`,
        );
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to create the group.`,
        );
      }
      this.saving.set(false);
    }
  }
}
