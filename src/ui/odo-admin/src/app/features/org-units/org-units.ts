import { Component, OnInit, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import { orgAdminApi, type TreeUnit, type UnitType } from './org-admin-api';
import { UnitDialog, type UnitDialogData } from './unit-dialog';
import { firstValueFrom } from 'rxjs';

/**
 * Org unit list: the organizational tree. The tree is the subject the user
 * drills into, so it is a framed table with clickable rows. Unit types (the
 * categories units belong to) are managed on their own dedicated page.
 */
@Component({
  selector: 'app-org-units',
  imports: [
    CdkTableModule,
    MatButtonModule,
    MatIconModule,
    MatProgressSpinnerModule,
    OdoTable,
  ],
  templateUrl: './org-units.html',
  styleUrl: './org-units.scss',
})
export class OrgUnits implements OnInit {
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly auth = inject(AuthService);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly unitColumns = ['label', 'code', 'type', 'timezone', 'actions'];

  protected readonly loading = signal(true);
  protected readonly units = signal<TreeUnit[]>([]);
  // Loaded for the create-unit dialog's type picker (not displayed here).
  private readonly unitTypes = signal<UnitType[]>([]);
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
      const [units, unitTypes] = await Promise.all([
        orgAdminApi.fetchTree(),
        orgAdminApi.listUnitTypes(),
      ]);
      this.units.set(units);
      this.unitTypes.set(unitTypes);
    } catch (err) {
      this.errors.show(err, 'Failed to load org units');
    } finally {
      this.loading.set(false);
    }
  }

  protected open(unit: TreeUnit): void {
    void this.router.navigate(['/org-units', unit.id]);
  }

  /** Create only — editing an existing unit happens on its detail page. */
  protected openUnitDialog(): void {
    const data: UnitDialogData = {
      units: this.units(),
      unitTypes: this.unitTypes(),
    };
    const ref = this.dialog.open(UnitDialog, { data, width: '520px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteUnit(unit: TreeUnit, event: Event): Promise<void> {
    event.stopPropagation();
    const confirmed = await this.confirm({
      title: $localize`Delete org unit?`,
      message: $localize`"${unit.label}" will be deactivated. Existing records that reference it are kept.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await orgAdminApi.deleteUnit(unit.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete org unit');
    }
  }

  private confirm(data: ConfirmDialogData): Promise<boolean> {
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    return firstValueFrom(ref.afterClosed()).then((result) => result === true);
  }
}
