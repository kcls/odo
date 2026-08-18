import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import {
  MAT_DIALOG_DATA,
  MatDialogModule,
  MatDialogRef,
} from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';

import { ApiRequestError } from '../../core/api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';
import { authzAdminApi, type PermissionRow } from '../roles/authz-api';

export interface PermissionDialogData {
  /** Present = edit (code immutable); absent = create. */
  permission?: PermissionRow;
}

/** Create/edit permission dialog; resolves with the saved permission. */
@Component({
  selector: 'app-permission-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
  ],
  template: `
    <h2 mat-dialog-title>
      @if (isEdit) {
        <ng-container i18n="Dialog title">Edit Permission</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Permission</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Permission field label">Code</mat-label>
          <input
            matInput
            name="code"
            [(ngModel)]="code"
            [disabled]="isEdit"
            required
            i18n-placeholder="Example permission code"
            placeholder="e.g. incident.report.read"
            [errorStateMatcher]="codeMatcher"
          />
          @if (isEdit) {
            <mat-hint i18n="Shown when editing a permission"
              >Codes cannot be changed once created.</mat-hint
            >
          } @else if (codeError()) {
            <mat-error>{{ codeError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Permission field label">Description</mat-label>
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
        [disabled]="saving() || !code.trim()"
        (click)="save()"
        i18n="Dialog action that saves the permission"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
})
export class PermissionDialog {
  private readonly dialogRef =
    inject<MatDialogRef<PermissionDialog, PermissionRow>>(MatDialogRef);
  private readonly data = inject<PermissionDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.permission;
  protected code = this.data.permission?.code ?? '';
  protected description = this.data.permission?.description ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly codeError = signal('');
  protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.codeError.set('');
    try {
      const params = {
        code: this.code.trim(),
        description: this.description.trim(),
      };
      const saved = this.isEdit
        ? await authzAdminApi.updatePermission(params)
        : await authzAdminApi.createPermission(params);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'PERMISSION_CODE_TAKEN') {
        this.codeError.set($localize`A permission with this code already exists.`);
      } else {
        this.error.set(
          err instanceof Error
            ? err.message
            : $localize`Failed to save the permission.`,
        );
      }
      this.saving.set(false);
    }
  }
}
