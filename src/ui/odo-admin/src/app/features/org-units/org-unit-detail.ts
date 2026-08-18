import {
  Component,
  computed,
  effect,
  inject,
  input,
  numberAttribute,
  signal,
} from '@angular/core';
import { DatePipe } from '@angular/common';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { firstValueFrom } from 'rxjs';

import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import {
  orgAdminApi,
  type OrgAddress,
  type OrgClosure,
  type OrgOperatingHours,
  type TreeUnit,
  type UnitType,
} from './org-admin-api';
import { AddressDialog, type AddressDialogData } from './address-dialog';
import { ClosureDialog, type ClosureDialogData } from './closure-dialog';
import { HoursDialog, type HoursDialogData } from './hours-dialog';
import { UnitDialog, type UnitDialogData } from './unit-dialog';

const DAY_NAMES = [
  $localize`:Day of week:Sunday`,
  $localize`:Day of week:Monday`,
  $localize`:Day of week:Tuesday`,
  $localize`:Day of week:Wednesday`,
  $localize`:Day of week:Thursday`,
  $localize`:Day of week:Friday`,
  $localize`:Day of week:Saturday`,
];

/**
 * Drill-down page for one org unit. Rather than tabbing between disconnected
 * lists, every facet of the unit — its child units, addresses, closures, and
 * operating hours — is a stacked section on one page, so the page reads as
 * "everything about this unit". Child units link deeper into the same page,
 * reinforcing the drill-down model.
 */
@Component({
  selector: 'app-org-unit-detail',
  imports: [
    DatePipe,
    CdkTableModule,
    MatButtonModule,
    MatIconModule,
    MatProgressSpinnerModule,
    DetailPage,
    OdoTable,
  ],
  templateUrl: './org-unit-detail.html',
  styleUrl: './org-unit-detail.scss',
})
export class OrgUnitDetail {
  /** Route param, bound via withComponentInputBinding. */
  readonly id = input.required({ transform: numberAttribute });

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly childColumns = ['label', 'code', 'type', 'open'];
  protected readonly addressColumns = ['label', 'type', 'lines', 'city', 'actions'];
  protected readonly closureColumns = ['date', 'reason', 'emergency', 'actions'];
  protected readonly hoursColumns = ['day', 'hours', 'actions'];

  protected readonly loading = signal(true);
  protected readonly unit = signal<TreeUnit | null>(null);
  protected readonly canWrite = signal(false);

  // Full flat tree + unit types are kept for the edit dialog's pickers.
  private units: TreeUnit[] = [];
  private unitTypes: UnitType[] = [];

  protected readonly childUnits = signal<TreeUnit[]>([]);
  protected readonly addresses = signal<OrgAddress[]>([]);
  protected readonly closures = signal<OrgClosure[]>([]);
  protected readonly hours = signal<OrgOperatingHours[]>([]);

  /** Summary line shown under the title in the detail-page chrome. */
  protected readonly typeLabel = computed(() => this.unit()?.unit_type.label ?? '');

  constructor() {
    this.auth.hasPerm(PERMS.ORG_UNIT_WRITE).then((ok) => this.canWrite.set(ok));

    // Reload whenever the route id changes. Navigating between org units
    // (e.g. clicking a child) reuses this component, so ngOnInit would only
    // fire once — an effect tracking id() reloads on every change, including
    // the initial one.
    effect(() => {
      const id = this.id();
      void this.reload(id);
    });
  }

  protected async reload(id: number = this.id()): Promise<void> {
    this.loading.set(true);
    try {
      const [units, unitTypes, children] = await Promise.all([
        orgAdminApi.fetchTree(),
        orgAdminApi.listUnitTypes(),
        orgAdminApi.unitChildren(id),
      ]);
      const unit = units.find((u) => u.id === id);
      if (!unit) {
        this.errors.show({ status: 404 }, 'Org unit not found');
        void this.router.navigate(['/org-units']);
        return;
      }
      this.units = units;
      this.unitTypes = unitTypes;
      this.unit.set(unit);
      this.childUnits.set(units.filter((u) => u.parent === unit.id));
      this.addresses.set(children.addresses);
      this.closures.set(children.closures);
      this.hours.set(children.operating_hours);
    } catch (err) {
      this.errors.show(err, 'Failed to load org unit');
    } finally {
      this.loading.set(false);
    }
  }

  protected dayName(day: number): string {
    return DAY_NAMES[day] ?? String(day);
  }

  protected shortTime(t: string): string {
    return t.slice(0, 5);
  }

  // --- Navigation ---

  protected openChild(child: TreeUnit): void {
    void this.router.navigate(['/org-units', child.id]);
  }

  // --- Unit ---

  protected editUnit(): void {
    const unit = this.unit();
    if (!unit) return;
    const data: UnitDialogData = { unit, units: this.units, unitTypes: this.unitTypes };
    const ref = this.dialog.open(UnitDialog, { data, width: '520px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  // --- Addresses ---

  protected addressDialog(address?: OrgAddress): void {
    const data: AddressDialogData = { orgUnit: this.id(), address };
    const ref = this.dialog.open(AddressDialog, { data, width: '520px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteAddress(address: OrgAddress): Promise<void> {
    if (!(await this.confirmDelete(address.label))) return;
    try {
      await orgAdminApi.deleteAddress(address.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete address');
    }
  }

  // --- Closures ---

  protected closureDialog(closure?: OrgClosure): void {
    const data: ClosureDialogData = { orgUnit: this.id(), closure };
    const ref = this.dialog.open(ClosureDialog, { data, width: '460px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteClosure(closure: OrgClosure): Promise<void> {
    if (!(await this.confirmDelete(closure.reason))) return;
    try {
      await orgAdminApi.deleteClosure(closure.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete closure');
    }
  }

  // --- Operating hours ---

  protected hoursDialog(hours?: OrgOperatingHours): void {
    const data: HoursDialogData = { orgUnit: this.id(), hours };
    const ref = this.dialog.open(HoursDialog, { data, width: '460px' });
    ref.afterClosed().subscribe((saved) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteHours(hours: OrgOperatingHours): Promise<void> {
    if (!(await this.confirmDelete(this.dayName(hours.day_of_week)))) return;
    try {
      await orgAdminApi.deleteHours(hours.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete hours');
    }
  }

  private confirmDelete(name: string): Promise<boolean> {
    const data: ConfirmDialogData = {
      title: $localize`Delete?`,
      message: $localize`"${name}" will be deleted.`,
      confirmLabel: $localize`Delete`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    return firstValueFrom(ref.afterClosed()).then((r) => r === true);
  }
}
