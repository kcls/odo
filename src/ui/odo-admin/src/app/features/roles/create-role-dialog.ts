import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';

import { ApiRequestError } from '../../core/api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';
import { authzAdminApi, type RoleRow } from './authz-api';

/** Create-role dialog; resolves with the new role on success. */
@Component({
  selector: 'app-create-role-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
  ],
  template: `
    <h2 mat-dialog-title i18n="Dialog title">New Role</h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Role field label">Code</mat-label>
          <input
            matInput
            name="code"
            [(ngModel)]="code"
            required
            i18n-placeholder="Example role code"
            placeholder="e.g. incident-viewer"
            [errorStateMatcher]="codeMatcher"
          />
          <mat-hint i18n="Help text for the role code field"
            >Unique identifier; cannot be changed later.</mat-hint
          >
          @if (codeError()) {
            <mat-error>{{ codeError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Role field label">Label</mat-label>
          <input
            matInput
            name="label"
            [(ngModel)]="label"
            required
            i18n-placeholder="Example role label"
            placeholder="e.g. Incident Viewer"
          />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Role field label">Description</mat-label>
          <input matInput name="description" [(ngModel)]="description" />
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
        i18n="Dialog action that creates the role"
      >
        Create
      </button>
    </mat-dialog-actions>
  `,
})
export class CreateRoleDialog {
  private readonly dialogRef =
    inject<MatDialogRef<CreateRoleDialog, RoleRow>>(MatDialogRef);

  protected code = '';
  protected label = '';
  protected description = '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly codeError = signal('');
  protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.codeError.set('');
    try {
      const role = await authzAdminApi.createRole({
        code: this.code.trim(),
        label: this.label.trim(),
        description: this.description.trim(),
      });
      this.dialogRef.close(role);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ROLE_CODE_TAKEN') {
        this.codeError.set($localize`A role with this code already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to create the role.`,
        );
      }
      this.saving.set(false);
    }
  }
}
