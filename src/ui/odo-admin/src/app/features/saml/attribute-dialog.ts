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
import {
  NORMALIZERS,
  samlAdminApi,
  type AttributeRow,
  type IdpRow,
} from './saml-api';

export interface AttributeDialogData {
  /** Present = edit (IdP immutable); absent = create. */
  attribute?: AttributeRow;
  idps: IdpRow[];
  /** Preselected IdP when creating from an IdP detail page. */
  defaultIdp?: number;
}

/** Create/edit SAML attribute dialog; resolves with the saved attribute. */
@Component({
  selector: 'app-attribute-dialog',
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
        <ng-container i18n="Dialog title">Edit SAML Attribute</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New SAML Attribute</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Attribute field label">Identity provider</mat-label>
          <mat-select name="idp" [(ngModel)]="idp" [disabled]="isEdit">
            @for (candidate of data.idps; track candidate.id) {
              <mat-option [value]="candidate.id">{{ candidate.name }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Attribute field label">Assertion key</mat-label>
          <input
            matInput
            name="key"
            [(ngModel)]="key"
            required
            i18n-placeholder="Example SAML assertion attribute name"
            placeholder="e.g. Title"
            [errorStateMatcher]="keyMatcher"
          />
          @if (keyError()) {
            <mat-error>{{ keyError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Attribute field label">Label</mat-label>
          <input matInput name="label" [(ngModel)]="label" required />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Attribute field label">Normalizer</mat-label>
          <mat-select name="normalizer" [(ngModel)]="normalizer">
            <mat-option value="" i18n="Option for no value normalizer"
              >None (raw value)</mat-option
            >
            @for (candidate of normalizers; track candidate) {
              <mat-option [value]="candidate">{{ candidate }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-slide-toggle name="isLocation" [(ngModel)]="isLocation" class="dialog-toggle">
          <ng-container i18n="Attribute toggle: value maps to a working location"
            >Maps to a working location</ng-container
          >
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
        [disabled]="saving() || idp === null || !key.trim() || !label.trim()"
        (click)="save()"
        i18n="Dialog action that saves the attribute"
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
export class AttributeDialog {
  private readonly dialogRef =
    inject<MatDialogRef<AttributeDialog, AttributeRow>>(MatDialogRef);
  protected readonly data = inject<AttributeDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.attribute;
  protected readonly normalizers = NORMALIZERS;

  protected idp: number | null =
    this.data.attribute?.idp ?? this.data.defaultIdp ?? null;
  protected key = this.data.attribute?.key ?? '';
  protected label = this.data.attribute?.label ?? '';
  protected isLocation = this.data.attribute?.is_location ?? false;
  protected normalizer = this.data.attribute?.normalizer ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly keyError = signal('');
  protected readonly keyMatcher = new ServerErrorStateMatcher(this.keyError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.keyError.set('');

    const params = {
      key: this.key.trim(),
      label: this.label.trim(),
      is_location: this.isLocation,
      normalizer: this.normalizer,
    };

    try {
      const saved = this.isEdit
        ? await samlAdminApi.updateAttribute(this.data.attribute!.id, params)
        : await samlAdminApi.createAttribute({ ...params, idp: this.idp! });
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ATTRIBUTE_EXISTS') {
        this.keyError.set(
          $localize`This IdP already tracks this key with the same normalizer.`,
        );
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the attribute.`,
        );
      }
      this.saving.set(false);
    }
  }
}
