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

import { ApiRequestError } from '../../core/api';
import {
  orgAdminApi,
  subtreeIds,
  type OrgUnitRow,
  type TreeUnit,
  type UnitType,
} from './org-admin-api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';

export interface UnitDialogData {
  /** Present = edit; absent = create. */
  unit?: TreeUnit;
  units: TreeUnit[];
  unitTypes: UnitType[];
}

/** Create/edit org unit dialog; resolves with the saved unit. */
@Component({
  selector: 'app-unit-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
  ],
  template: `
    <h2 mat-dialog-title>
      @if (isEdit) {
        <ng-container i18n="Dialog title">Edit Org Unit</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Org Unit</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Org unit field label">Label</mat-label>
          <input
            matInput
            name="label"
            [(ngModel)]="label"
            [errorStateMatcher]="labelMatcher"
            required
          />
          @if (labelError()) {
            <mat-error>{{ labelError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Org unit field label">Code</mat-label>
          <input
            matInput
            name="code"
            [(ngModel)]="code"
            [errorStateMatcher]="codeMatcher"
            required
          />
          @if (codeError()) {
            <mat-error>{{ codeError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Org unit field label">Parent</mat-label>
          <mat-select name="parent" [(ngModel)]="parent" [disabled]="isRoot">
            @for (candidate of parentOptions; track candidate.id) {
              <mat-option [value]="candidate.id">
                <span [style.padding-left.px]="candidate.depth * 16">{{
                  candidate.label
                }}</span>
              </mat-option>
            }
          </mat-select>
          @if (isRoot) {
            <mat-hint i18n="Hint shown when editing the root unit"
              >The root unit cannot be moved.</mat-hint
            >
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Org unit field label">Type</mat-label>
          <mat-select name="unitType" [(ngModel)]="unitType">
            @for (candidate of data.unitTypes; track candidate.id) {
              <mat-option [value]="candidate.id">{{ candidate.label }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Org unit field label">Timezone</mat-label>
          <input
            matInput
            name="timezone"
            [(ngModel)]="timezone"
            i18n-placeholder="Example IANA timezone"
            placeholder="e.g. America/Los_Angeles"
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
        [disabled]="
          saving() ||
          !label.trim() ||
          !code.trim() ||
          unitType === null ||
          (!isRoot && parent === null)
        "
        (click)="save()"
        i18n="Dialog action that saves the org unit"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
})
export class UnitDialog {
  private readonly dialogRef =
    inject<MatDialogRef<UnitDialog, OrgUnitRow>>(MatDialogRef);
  protected readonly data = inject<UnitDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.unit;
  protected readonly isRoot = !!this.data.unit && this.data.unit.parent === null;

  /** When editing, a unit cannot move under itself or its descendants. */
  protected readonly parentOptions: TreeUnit[] = (() => {
    if (!this.data.unit) return this.data.units;
    const excluded = subtreeIds(this.data.units, this.data.unit.id);
    return this.data.units.filter((u) => !excluded.has(u.id));
  })();

  protected label = this.data.unit?.label ?? '';
  protected code = this.data.unit?.code ?? '';
  protected parent: number | null = this.data.unit?.parent ?? null;
  protected unitType: number | null = this.data.unit?.unit_type.id ?? null;
  protected timezone = this.data.unit?.timezone ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly labelError = signal('');
  protected readonly codeError = signal('');
  protected readonly labelMatcher = new ServerErrorStateMatcher(this.labelError);
  protected readonly codeMatcher = new ServerErrorStateMatcher(this.codeError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.labelError.set('');
    this.codeError.set('');

    const label = this.label.trim();
    const code = this.code.trim();
    const timezone = this.timezone.trim();

    try {
      let saved: OrgUnitRow;
      if (this.isEdit) {
        // Update is a partial: the root keeps its NULL parent (the API
        // rejects moving it anyway), so only send parent when it moves.
        saved = await orgAdminApi.updateUnit(this.data.unit!.id, {
          label,
          code,
          unit_type: this.unitType!,
          timezone,
          ...(!this.isRoot && this.parent !== null ? { parent: this.parent } : {}),
        });
      } else {
        // Create requires a parent; the Save button is disabled until one is
        // picked, so this non-null assertion is guarded by the template.
        saved = await orgAdminApi.createUnit({
          label,
          code,
          unit_type: this.unitType!,
          timezone,
          parent: this.parent!,
        });
      }
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'UNIT_CODE_TAKEN') {
        this.codeError.set($localize`An org unit with this code already exists.`);
      } else if (err instanceof ApiRequestError && err.code === 'UNIT_LABEL_TAKEN') {
        this.labelError.set($localize`An org unit with this label already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the org unit.`,
        );
      }
      this.saving.set(false);
    }
  }
}
