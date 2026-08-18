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
import { type RoleRow } from '../roles/authz-api';
import {
  samlAdminApi,
  type AttrRoleMapRow,
  type AttributeRow,
} from './saml-api';

export interface MappingDialogData {
  /** Present = edit; absent = create. */
  mapping?: AttrRoleMapRow;
  attributes: AttributeRow[];
  roles: RoleRow[];
}

/** Create/edit attribute-to-role mapping dialog; resolves with the mapping. */
@Component({
  selector: 'app-mapping-dialog',
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
        <ng-container i18n="Dialog title">Edit Role Mapping</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Role Mapping</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Mapping field label">Attribute</mat-label>
          <mat-select name="attr" [(ngModel)]="attr">
            @for (candidate of data.attributes; track candidate.id) {
              <mat-option [value]="candidate.id"
                >{{ candidate.key }} ({{ candidate.idp_name }})</mat-option
              >
            }
          </mat-select>
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Mapping field label">Attribute value</mat-label>
          <input
            matInput
            name="attrValue"
            [(ngModel)]="attrValue"
            required
            i18n-placeholder="Example SAML attribute value"
            placeholder="e.g. Operations Manager"
            [errorStateMatcher]="attrValueMatcher"
          />
          @if (valueError()) {
            <mat-error>{{ valueError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Mapping field label">Role</mat-label>
          <mat-select name="role" [(ngModel)]="role">
            @for (candidate of data.roles; track candidate.code) {
              <mat-option [value]="candidate.code">{{ candidate.label }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-slide-toggle name="isActive" [(ngModel)]="isActive" class="dialog-toggle">
          <ng-container i18n="Mapping toggle label">Active</ng-container>
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
        [disabled]="saving() || attr === null || !role || !attrValue.trim()"
        (click)="save()"
        i18n="Dialog action that saves the mapping"
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
export class MappingDialog {
  private readonly dialogRef =
    inject<MatDialogRef<MappingDialog, AttrRoleMapRow>>(MatDialogRef);
  protected readonly data = inject<MappingDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.mapping;

  protected attr: number | null = this.data.mapping?.attr ?? null;
  protected attrValue = this.data.mapping?.attr_value ?? '';
  protected role = this.data.mapping?.role ?? '';
  protected isActive = this.data.mapping?.is_active ?? true;

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly valueError = signal('');
  protected readonly attrValueMatcher = new ServerErrorStateMatcher(this.valueError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.valueError.set('');

    const params = {
      attr: this.attr!,
      role: this.role,
      attr_value: this.attrValue.trim(),
      is_active: this.isActive,
    };

    try {
      const saved = this.isEdit
        ? await samlAdminApi.updateAttrRoleMap(this.data.mapping!.id, params)
        : await samlAdminApi.createAttrRoleMap(params);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'MAPPING_EXISTS') {
        this.valueError.set(
          $localize`This attribute value is already mapped to this role.`,
        );
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the mapping.`,
        );
      }
      this.saving.set(false);
    }
  }
}
