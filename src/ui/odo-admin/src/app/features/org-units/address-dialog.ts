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
import { orgAdminApi, type OrgAddress } from './org-admin-api';
import { ServerErrorStateMatcher } from '../../shared/server-error-state-matcher';

export interface AddressDialogData {
  orgUnit: number;
  address?: OrgAddress;
}

/** Create/edit an org unit address; resolves with the saved address. */
@Component({
  selector: 'app-address-dialog',
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
        <ng-container i18n="Dialog title">Edit Address</ng-container>
      } @else {
        <ng-container i18n="Dialog title">New Address</ng-container>
      }
    </h2>
    <mat-dialog-content>
      <div class="dialog-form">
        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">Label</mat-label>
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
          <mat-label i18n="Address field label">Type</mat-label>
          <mat-select name="addressType" [(ngModel)]="addressType">
            <mat-option value="physical" i18n="Address type option">Physical</mat-option>
            <mat-option value="mailing" i18n="Address type option">Mailing</mat-option>
          </mat-select>
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">Address line 1</mat-label>
          <input matInput name="line1" [(ngModel)]="line1" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">Address line 2</mat-label>
          <input matInput name="line2" [(ngModel)]="line2" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">City</mat-label>
          <input matInput name="city" [(ngModel)]="city" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">State / province</mat-label>
          <input matInput name="stateProvince" [(ngModel)]="stateProvince" />
        </mat-form-field>

        <mat-form-field appearance="outline" class="dialog-field">
          <mat-label i18n="Address field label">Postal code</mat-label>
          <input matInput name="postalCode" [(ngModel)]="postalCode" />
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
        [disabled]="saving() || !label.trim()"
        (click)="save()"
        i18n="Dialog action that saves the address"
      >
        Save
      </button>
    </mat-dialog-actions>
  `,
})
export class AddressDialog {
  private readonly dialogRef =
    inject<MatDialogRef<AddressDialog, OrgAddress>>(MatDialogRef);
  private readonly data = inject<AddressDialogData>(MAT_DIALOG_DATA);

  protected readonly isEdit = !!this.data.address;
  protected label = this.data.address?.label ?? '';
  protected addressType = this.data.address?.address_type ?? 'physical';
  protected line1 = this.data.address?.address_line1 ?? '';
  protected line2 = this.data.address?.address_line2 ?? '';
  protected city = this.data.address?.city ?? '';
  protected stateProvince = this.data.address?.state_province ?? '';
  protected postalCode = this.data.address?.postal_code ?? '';

  protected readonly saving = signal(false);
  protected readonly error = signal('');
  protected readonly labelError = signal('');
  protected readonly labelMatcher = new ServerErrorStateMatcher(this.labelError);

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.error.set('');
    this.labelError.set('');
    const params = {
      address_type: this.addressType,
      label: this.label.trim(),
      address_line1: this.line1.trim(),
      address_line2: this.line2.trim(),
      city: this.city.trim(),
      state_province: this.stateProvince.trim(),
      postal_code: this.postalCode.trim(),
    };
    try {
      const saved = this.isEdit
        ? await orgAdminApi.updateAddress(this.data.address!.id, params)
        : await orgAdminApi.createAddress(this.data.orgUnit, params);
      this.dialogRef.close(saved);
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ADDRESS_LABEL_TAKEN') {
        this.labelError.set($localize`An address with this label already exists.`);
      } else {
        this.error.set(
          err instanceof Error ? err.message : $localize`Failed to save the address.`,
        );
      }
      this.saving.set(false);
    }
  }
}
