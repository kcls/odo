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
import { MatSelectModule } from '@angular/material/select';
import { MatSlideToggleModule } from '@angular/material/slide-toggle';

import { orgAdminApi, type OrgOperatingHours } from './org-admin-api';

export interface HoursDialogData {
  orgUnit: number;
  hours?: OrgOperatingHours;
}

const DAYS = [
  $localize`:Day of week:Sunday`,
  $localize`:Day of week:Monday`,
  $localize`:Day of week:Tuesday`,
  $localize`:Day of week:Wednesday`,
  $localize`:Day of week:Thursday`,
  $localize`:Day of week:Friday`,
  $localize`:Day of week:Saturday`,
];

/** Create/edit an operating-hours row; resolves with the saved row. */
@Component({
  selector: 'app-hours-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatSlideToggleModule,
  ],
  template: `
    <h2 mat-dialog-title>
      @if (isEdit) {
        <ng-container i18n="Dialog title">Edit Hours</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Hours</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Operating hours field label">Day</mat-label>
          <mat-select name="day" [(ngModel)]="dayOfWeek">
            @for (day of days; track $index) {
              <mat-option [value]="$index">{{ day }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-slide-toggle name="closed" [(ngModel)]="isClosed" class="dialog-toggle">
          <ng-container i18n="Operating hours toggle label">Closed all day</ng-container>
        </mat-slide-toggle>

        @if (!isClosed) {
          <mat-form-field appearance="outline" class="dialog-field">
            <mat-label i18n="Operating hours field label">Open time</mat-label>
            <input matInput type="time" name="open" [(ngModel)]="openTime" />
          </mat-form-field>

          <mat-form-field appearance="outline" class="dialog-field">
            <mat-label i18n="Operating hours field label">Close time</mat-label>
            <input matInput type="time" name="close" [(ngModel)]="closeTime" />
          </mat-form-field>
        }

        @if (error()) {
          <p class="dialog-error" role="alert">{{ error() }}</p>
        }
      </div>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button mat-dialog-close i18n="Dialog dismiss button">Cancel</button>
      <button
        mat-flat-button
        [disabled]="saving() || !canSave()"
        (click)="save()"
        i18n="Dialog action that saves the hours"
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
export class HoursDialog {
  private readonly dialogRef =
    inject<MatDialogRef<HoursDialog, OrgOperatingHours>>(MatDialogRef);
  private readonly data = inject<HoursDialogData>(MAT_DIALOG_DATA);

  protected readonly days = DAYS;
  protected readonly isEdit = !!this.data.hours;
  protected dayOfWeek = this.data.hours?.day_of_week ?? 1;
  protected openTime = (this.data.hours?.open_time ?? '09:00:00').slice(0, 5);
  protected closeTime = (this.data.hours?.close_time ?? '17:00:00').slice(0, 5);
  protected isClosed = this.data.hours?.is_closed ?? false;

  protected readonly saving = signal(false);
  protected readonly error = signal('');

  protected canSave(): boolean {
    return this.isClosed || (!!this.openTime && !!this.closeTime);
  }

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    const params = {
      day_of_week: this.dayOfWeek,
      open_time: `${this.openTime}:00`,
      close_time: `${this.closeTime}:00`,
      is_closed: this.isClosed,
    };
    try {
      const saved = this.isEdit
        ? await orgAdminApi.updateHours(this.data.hours!.id, params)
        : await orgAdminApi.createHours(this.data.orgUnit, params);
      this.dialogRef.close(saved);
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : $localize`Failed to save the hours.`,
      );
      this.saving.set(false);
    }
  }
}
