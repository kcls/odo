import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import {
  MAT_DIALOG_DATA,
  MatDialogModule,
  MatDialogRef,
} from '@angular/material/dialog';
import { MatDatepickerModule } from '@angular/material/datepicker';
import { provideNativeDateAdapter } from '@angular/material/core';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatInputModule } from '@angular/material/input';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';

import { orgAdminApi, type OrgClosure } from './org-admin-api';

export interface ClosureDialogData {
  orgUnit: number;
  closure?: OrgClosure;
}

/** Create/edit an org unit closure; resolves with the saved closure. */
@Component({
  selector: 'app-closure-dialog',
  providers: [provideNativeDateAdapter()],
  imports: [
    FormsModule,
    MatButtonModule,
    MatDatepickerModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSlideToggleModule,
  ],
  template: `
    <h2 mat-dialog-title>
      @if (isEdit) {
        <ng-container i18n="Dialog title">Edit Closure</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Closure</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Closure field label">Date</mat-label>
          <input matInput [matDatepicker]="picker" name="date" [(ngModel)]="date" />
          <mat-datepicker-toggle matIconSuffix [for]="picker" />
          <mat-datepicker #picker />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Closure field label">Reason</mat-label>
          <input matInput name="reason" [(ngModel)]="reason" required />
        </mat-form-field>

        <mat-slide-toggle name="emergency" [(ngModel)]="isEmergency" class="dialog-toggle">
          <ng-container i18n="Closure toggle label">Emergency closure</ng-container>
        </mat-slide-toggle>

        @if (error()) {
          <p class="dialog-error" role="alert">{{ error() }}</p>
        }
      </div>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button mat-dialog-close i18n="Dialog dismiss button">Cancel</button>
      <button
        mat-flat-button
        [disabled]="saving() || !date || !reason.trim()"
        (click)="save()"
        i18n="Dialog action that saves the closure"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
  styles: `
    .dialog-toggle {
      display: block;
      margin-bottom: 20px;
    }
  `,
})
export class ClosureDialog {
  private readonly dialogRef =
    inject<MatDialogRef<ClosureDialog, OrgClosure>>(MatDialogRef);
  private readonly data = inject<ClosureDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.closure;
  protected date: Date | null = this.data.closure
    ? new Date(`${this.data.closure.closure_date}T00:00:00`)
    : null;
  protected reason = this.data.closure?.reason ?? '';
  protected isEmergency = this.data.closure?.is_emergency ?? false;

  protected readonly saving = signal(false);
  protected readonly error = signal('');

  private isoDate(d: Date): string {
    // Local calendar date, not UTC, so a picked date isn't shifted a day.
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${day}`;
  }

  protected async save(): Promise<void> {
    if (!this.date) return;
    this.saving.set(true);
    this.error.set('');
    const params = {
      closure_date: this.isoDate(this.date),
      reason: this.reason.trim(),
      is_emergency: this.isEmergency,
    };
    try {
      const saved = this.isEdit
        ? await orgAdminApi.updateClosure(this.data.closure!.id, params)
        : await orgAdminApi.createClosure(this.data.orgUnit, params);
      this.dialogRef.close(saved);
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : $localize`Failed to save the closure.`,
      );
      this.saving.set(false);
    }
  }
}
