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
import { MatSlideToggleModule } from '@angular/material/slide-toggle';

import { ApiRequestError } from '../../core/api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';
import { samlAdminApi, type CreateIdpRequest, type IdpRow } from './saml-api';

export interface IdpDialogData {
  /** Present = edit; absent = create. */
  idp?: IdpRow;
}

/** Create/edit SAML IdP config dialog; resolves with the saved config. */
@Component({
  selector: 'app-idp-dialog',
  imports: [
    FormsModule,
    MatButtonModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSlideToggleModule,
  ],
  template: `
    <h2 mat-dialog-title>
      @if (isEdit) {
        <ng-container i18n="Dialog title">Edit Identity Provider</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Identity Provider</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">Name</mat-label>
          <input matInput name="name" [(ngModel)]="name" required />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">Entity ID</mat-label>
          <input matInput name="entityId" [(ngModel)]="entityId" required
            [errorStateMatcher]="entityIdMatcher" />
          @if (entityIdError()) {
            <mat-error>{{ entityIdError() }}</mat-error>
          }
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">SSO URL</mat-label>
          <input matInput name="ssoUrl" [(ngModel)]="ssoUrl" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">SLO URL</mat-label>
          <input matInput name="sloUrl" [(ngModel)]="sloUrl" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">Metadata URL</mat-label>
          <input matInput name="metadataUrl" [(ngModel)]="metadataUrl" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field-half">
          <mat-label i18n="IdP field label">Session lifetime (hours)</mat-label>
          <input
            matInput
            type="number"
            min="1"
            name="sessionLifetime"
            [(ngModel)]="sessionLifetimeHours"
          />
        </mat-form-field>

        <div class="dialog-toggles">
          <mat-slide-toggle name="allowIdpInit" [(ngModel)]="allowIdpInitiated"
            ><ng-container i18n="IdP toggle label"
              >Allow IdP-initiated login</ng-container
            ></mat-slide-toggle
          >
          <mat-slide-toggle name="isActive" [(ngModel)]="isActive"
            ><ng-container i18n="IdP toggle label">Active</ng-container></mat-slide-toggle
          >
        </div>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="IdP field label">Attribute mapping (JSON)</mat-label>
          <textarea
            matInput
            name="attributeMapping"
            rows="4"
            [(ngModel)]="attributeMapping"
          ></textarea>
          @if (mappingError()) {
            <mat-error>{{ mappingError() }}</mat-error>
          }
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
        [disabled]="saving() || !name.trim() || !entityId.trim()"
        (click)="save()"
        i18n="Dialog action that saves the IdP config"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
  styles: `
    .dialog-field-half {
      width: 240px;
      margin-bottom: 16px;
    }
    .dialog-toggles {
      display: flex;
      gap: 24px;
      margin-bottom: 20px;
    }
  `,
})
export class IdpDialog {
  private readonly dialogRef =
    inject<MatDialogRef<IdpDialog, IdpRow>>(MatDialogRef);
  private readonly data = inject<IdpDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.idp;
  protected name = this.data.idp?.name ?? '';
  protected entityId = this.data.idp?.entity_id ?? '';
  protected ssoUrl = this.data.idp?.sso_url ?? '';
  protected sloUrl = this.data.idp?.slo_url ?? '';
  protected metadataUrl = this.data.idp?.metadata_url ?? '';
  protected sessionLifetimeHours: number | null =
    this.data.idp?.session_lifetime_hours ?? null;
  protected allowIdpInitiated = this.data.idp?.allow_idp_initiated ?? false;
  protected isActive = this.data.idp?.is_active ?? true;
  protected attributeMapping = this.data.idp?.attribute_mapping
    ? JSON.stringify(this.data.idp.attribute_mapping, null, 2)
    : '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly entityIdError = signal('');
  protected readonly entityIdMatcher = new ServerErrorStateMatcher(this.entityIdError);
  protected readonly mappingError = signal('');

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.entityIdError.set('');
    this.mappingError.set('');

    let mapping: unknown;
    const mappingText = this.attributeMapping.trim();
    if (mappingText) {
      try {
        mapping = JSON.parse(mappingText);
      } catch {
        this.mappingError.set($localize`Attribute mapping must be valid JSON.`);
        this.saving.set(false);
        return;
      }
    }

    const params: CreateIdpRequest = {
      name: this.name.trim(),
      entity_id: this.entityId.trim(),
      sso_url: this.ssoUrl.trim(),
      slo_url: this.sloUrl.trim(),
      metadata_url: this.metadataUrl.trim(),
      is_active: this.isActive,
      allow_idp_initiated: this.allowIdpInitiated,
      ...(this.sessionLifetimeHours !== null
        ? { session_lifetime_hours: this.sessionLifetimeHours }
        : {}),
      ...(mapping !== undefined ? { attribute_mapping: mapping } : {}),
    };

    try {
      const saved = this.isEdit
        ? await samlAdminApi.updateIdp(this.data.idp!.id, params)
        : await samlAdminApi.createIdp(params);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ENTITY_ID_TAKEN') {
        this.entityIdError.set($localize`A config with this entity ID already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the IdP config.`,
        );
      }
      this.saving.set(false);
    }
  }
}
