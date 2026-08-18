import { Component, OnInit, inject, signal } from '@angular/core';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { firstValueFrom } from 'rxjs';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import { orgAdminApi, type UnitType } from '../org-units/org-admin-api';
import { UnitTypeDialog, type UnitTypeDialogData } from './unit-type-dialog';

/**
 * Org unit types: the categories of org units (system, region, branch, …) and
 * what each may contain. Reference data for the org tree — a small bounded list,
 * so it loads whole (no pagination).
 */
@Component({
  selector: 'app-org-unit-types',
  imports: [
    CdkTableModule,
    MatButtonModule,
    MatIconModule,
    MatProgressSpinnerModule,
    OdoTable,
  ],
  templateUrl: './org-unit-types.html',
  styleUrl: './org-unit-types.scss',
})
export class OrgUnitTypes implements OnInit {
  private readonly dialog = inject(MatDialog);
  private readonly auth = inject(AuthService);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly typeColumns = ['label', 'parent', 'capabilities', 'units', 'actions'];

  protected readonly loading = signal(true);
  protected readonly unitTypes = signal<UnitType[]>([]);
  protected readonly canWrite = signal(false);

  constructor() {
    this.auth.hasPerm(PERMS.ORG_UNIT_WRITE).then((ok) => this.canWrite.set(ok));
  }

  ngOnInit(): void {
    void this.reload();
  }

  protected async reload(): Promise<void> {
    this.loading.set(true);
    try {
      this.unitTypes.set(await orgAdminApi.listUnitTypes());
    } catch (err) {
      this.errors.show(err, 'Failed to load unit types');
    } finally {
      this.loading.set(false);
    }
  }

  protected openTypeDialog(unitType?: UnitType): void {
    const data: UnitTypeDialogData = { unitType, unitTypes: this.unitTypes() };
    const ref = this.dialog.open(UnitTypeDialog, { data, width: '480px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteType(unitType: UnitType): Promise<void> {
    const confirmed = await this.confirm({
      title: $localize`Delete unit type?`,
      message: $localize`"${unitType.label}" will be deactivated.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await orgAdminApi.deleteUnitType(unitType.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete unit type');
    }
  }

  private confirm(data: ConfirmDialogData): Promise<boolean> {
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    return firstValueFrom(ref.afterClosed()).then((result) => result === true);
  }
}
