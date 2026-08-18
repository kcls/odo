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

import { ApiRequestError } from '../../core/api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';
import { orgAdminApi, type UnitType } from '../org-units/org-admin-api';

export interface UnitTypeDialogData {
  /** Present = edit; absent = create. */
  unitType?: UnitType;
  unitTypes: UnitType[];
}

/** Create/edit org unit type dialog; resolves with the saved type. */
@Component({
  selector: 'app-unit-type-dialog',
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
        <ng-container i18n="Dialog title">Edit Unit Type</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Unit Type</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Unit type field label">Label</mat-label>
          <input matInput name="label" [(ngModel)]="label" required
            [errorStateMatcher]="labelMatcher" />
          @if (labelError()) {
            <mat-error>{{ labelError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Unit type field label">Parent type</mat-label>
          <mat-select name="parent" [(ngModel)]="parent">
            <mat-option [value]="null" i18n="Option for no parent type">None</mat-option>
            @for (candidate of parentOptions; track candidate.id) {
              <mat-option [value]="candidate.id">{{ candidate.label }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <div class="dialog-toggles">
          <mat-slide-toggle name="canHaveStaff" [(ngModel)]="canHaveStaff"
            ><ng-container i18n="Unit type toggle label"
              >Can have staff</ng-container
            ></mat-slide-toggle
          >
          <mat-slide-toggle name="canHavePatrons" [(ngModel)]="canHavePatrons"
            ><ng-container i18n="Unit type toggle label"
              >Can have patrons</ng-container
            ></mat-slide-toggle
          >
        </div>

        @if (error()) {
          <p class="dialog-error" role="alert">{{ error() }}</p>
        }
      </div>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button mat-dialog-close i18n="Dialog dismiss button">Cancel</button>
      <button
        mat-flat-button
        [disabled]="saving() || !label.trim()"
        (click)="save()"
        i18n="Dialog action that saves the unit type"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
  styles: `
    .dialog-toggles {
      display: flex;
      gap: 24px;
      margin-bottom: 20px;
    }
  `,
})
export class UnitTypeDialog {
  private readonly dialogRef =
    inject<MatDialogRef<UnitTypeDialog, UnitType>>(MatDialogRef);
  protected readonly data = inject<UnitTypeDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.unitType;

  /** A type cannot be its own parent; deeper cycles are caught server-side. */
  protected readonly parentOptions: UnitType[] = this.data.unitTypes.filter(
    (t) => t.id !== this.data.unitType?.id,
  );

  protected label = this.data.unitType?.label ?? '';
  protected parent: number | null = this.data.unitType?.parent ?? null;
  protected canHaveStaff = this.data.unitType?.can_have_staff ?? false;
  protected canHavePatrons = this.data.unitType?.can_have_patrons ?? false;

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly labelError = signal('');
  protected readonly labelMatcher = new ServerErrorStateMatcher(this.labelError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.labelError.set('');

    const params = {
      label: this.label.trim(),
      can_have_staff: this.canHaveStaff,
      can_have_patrons: this.canHavePatrons,
      ...(this.parent !== null ? { parent: this.parent } : {}),
    };

    try {
      const saved = this.isEdit
        ? await orgAdminApi.updateUnitType(this.data.unitType!.id, params)
        : await orgAdminApi.createUnitType(params);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'TYPE_LABEL_TAKEN') {
        this.labelError.set($localize`A unit type with this label already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the unit type.`,
        );
      }
      this.saving.set(false);
    }
  }
}
