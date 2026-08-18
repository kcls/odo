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
  samlAdminApi,
  type CreateSpRequest,
  type IdpRow,
  type SpRow,
} from './saml-api';

export interface SpDialogData {
  /** Present = edit; absent = create. */
  sp?: SpRow;
  idps: IdpRow[];
  /** Preselected IdP when creating from an IdP detail page. */
  defaultIdp?: number;
}

/** Create/edit SAML SP config dialog; resolves with the saved config. */
@Component({
  selector: 'app-sp-dialog',
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
        <ng-container i18n="Dialog title">Edit Service Provider</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Service Provider</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">Label</mat-label>
          <input matInput name="label" [(ngModel)]="label" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">Entity ID</mat-label>
          <input matInput name="entityId" [(ngModel)]="entityId" required
            [errorStateMatcher]="entityIdMatcher" />
          @if (entityIdError()) {
            <mat-error>{{ entityIdError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">ACS URL</mat-label>
          <input matInput name="acsUrl" [(ngModel)]="acsUrl" required />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">SLO URL</mat-label>
          <input matInput name="sloUrl" [(ngModel)]="sloUrl" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">Callback URL</mat-label>
          <input matInput name="callbackUrl" [(ngModel)]="callbackUrl" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">Identity provider</mat-label>
          <mat-select name="idp" [(ngModel)]="idp">
            <mat-option [value]="null" i18n="Option for no identity provider"
              >None</mat-option
            >
            @for (candidate of data.idps; track candidate.id) {
              <mat-option [value]="candidate.id">{{ candidate.name }}</mat-option>
            }
          </mat-select>
        </mat-form-field>

        <mat-slide-toggle name="isActive" [(ngModel)]="isActive" class="dialog-toggle"
          ><ng-container i18n="SP toggle label">Active</ng-container></mat-slide-toggle
        >

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">SP certificate (x509)</mat-label>
          <textarea matInput name="x509Cert" rows="4" [(ngModel)]="x509Cert"></textarea>
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">SP private key</mat-label>
          <textarea
            matInput
            name="privateKey"
            rows="4"
            [(ngModel)]="privateKey"
            i18n-placeholder="Placeholder shown when editing an SP with a stored key"
            [placeholder]="isEdit ? keepKeyPlaceholder : ''"
          ></textarea>
          @if (isEdit) {
            <mat-hint i18n="Hint for the private key field when editing"
              >Leave blank to keep the stored key. The key is never displayed.</mat-hint
            >
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="SP field label">IdP certificate (x509)</mat-label>
          <textarea
            matInput
            name="idpX509Cert"
            rows="4"
            [(ngModel)]="idpX509Cert"
          ></textarea>
        </mat-form-field>

        @if (error()) {
          <p class="dialog-error" role="alert">{{ error() }}</p>
        }
      </div>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button mat-dialog-close i18n="Dialog dismiss button">Cancel</button>
      <button mat-flat-button [disabled]="!canSave()" (click)="save()" i18n="Dialog action that saves the SP config">
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
export class SpDialog {
  private readonly dialogRef = inject<MatDialogRef<SpDialog, SpRow>>(MatDialogRef);
  protected readonly data = inject<SpDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.sp;
  protected readonly keepKeyPlaceholder = $localize`(unchanged)`;

  protected label = this.data.sp?.label ?? '';
  protected entityId = this.data.sp?.entity_id ?? '';
  protected acsUrl = this.data.sp?.acs_url ?? '';
  protected sloUrl = this.data.sp?.slo_url ?? '';
  protected callbackUrl = this.data.sp?.callback_url ?? '';
  protected idp: number | null =
    this.data.sp?.idp ?? this.data.defaultIdp ?? null;
  protected isActive = this.data.sp?.is_active ?? true;
  protected x509Cert = this.data.sp?.x509_cert ?? '';
  // Write-only: the API never returns the private key, so it always starts empty.
  protected privateKey = '';
  protected idpX509Cert = this.data.sp?.idp_x509_cert ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly entityIdError = signal('');
  protected readonly entityIdMatcher = new ServerErrorStateMatcher(this.entityIdError);

  protected canSave(): boolean {
    if (this.saving() || !this.entityId.trim() || !this.acsUrl.trim()) return false;
    // Creation needs cert + key; edits may leave them untouched.
    if (!this.isEdit && (!this.x509Cert.trim() || !this.privateKey.trim())) {
      return false;
    }
    return true;
  }

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.entityIdError.set('');

    // Partial: entity_id/acs_url are required for create (guarded by canSave),
    // and left absent on update only when blank. The private key is write-only:
    // sent only when the operator typed one, so a blank edit keeps the stored key.
    const params: Partial<CreateSpRequest> = {
      entity_id: this.entityId.trim(),
      label: this.label.trim(),
      acs_url: this.acsUrl.trim(),
      slo_url: this.sloUrl.trim(),
      callback_url: this.callbackUrl.trim(),
      is_active: this.isActive,
      idp_x509_cert: this.idpX509Cert.trim(),
      ...(this.idp !== null ? { idp: this.idp } : {}),
      ...(this.x509Cert.trim() ? { x509_cert: this.x509Cert.trim() } : {}),
      ...(this.privateKey.trim() ? { private_key: this.privateKey.trim() } : {}),
    };

    try {
      const saved = this.isEdit
        ? await samlAdminApi.updateSp(this.data.sp!.id, params)
        : await samlAdminApi.createSp(params as CreateSpRequest);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ENTITY_ID_TAKEN') {
        this.entityIdError.set($localize`A config with this entity ID already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the SP config.`,
        );
      }
      this.saving.set(false);
    }
  }
}
