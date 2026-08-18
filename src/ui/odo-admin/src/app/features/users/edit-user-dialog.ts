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

import { userAdminApi, type UserAccount } from './user-api';

export interface EditUserDialogData {
  user: UserAccount;
}

/** Edit a local user's name fields; resolves with the updated account. */
@Component({
  selector: 'app-edit-user-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
  ],
  template: `
    <h2 mat-dialog-title i18n="Dialog title">Edit User</h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="User field label">First name</mat-label>
          <input matInput name="firstName" [(ngModel)]="firstName" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="User field label">Middle name</mat-label>
          <input matInput name="secondName" [(ngModel)]="secondName" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="User field label">Family name</mat-label>
          <input matInput name="familyName" [(ngModel)]="familyName" />
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
        [disabled]="saving()"
        (click)="save()"
        i18n="Dialog action that saves the user"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
})
export class EditUserDialog {
  private readonly dialogRef =
    inject<MatDialogRef<EditUserDialog, UserAccount>>(MatDialogRef);
  private readonly data = inject<EditUserDialogData>(MAT_DIALOG_DATA);

  protected firstName = this.data.user.first_given_name ?? '';
  protected secondName = '';
  protected familyName = this.data.user.family_name ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    try {
      const updated = await userAdminApi.updateUser(this.data.user.id, {
        first_given_name: this.firstName.trim(),
        second_given_name: this.secondName.trim(),
        family_name: this.familyName.trim(),
      });
      this.dialogRef.close(updated);
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : $localize`Failed to save the user.`,
      );
      this.saving.set(false);
    }
  }
}
