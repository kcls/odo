import {
  Component,
  computed,
  effect,
  inject,
  input,
  numberAttribute,
  signal,
} from '@angular/core';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { firstValueFrom } from 'rxjs';

import { ApiRequestError } from '../../core/api';
import { AuthService } from '../../core/auth.service';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { PERMS } from '../../core/perms';
import { authzAdminApi, type RoleRow } from '../roles/authz-api';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { OdoTable, type OdoSort } from '../../shared/odo-table/odo-table';
import { OdoSortHeader } from '../../shared/odo-table/odo-sort-header';
import {
  OdoPaginator,
  type OdoPageEvent,
} from '../../shared/odo-paginator/odo-paginator';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import {
  samlAdminApi,
  type AttrRoleMapRow,
  type AttributeRow,
  type IdpRow,
  type SpRow,
} from './saml-api';
import { AttributeDialog, type AttributeDialogData } from './attribute-dialog';
import { IdpDialog, type IdpDialogData } from './idp-dialog';
import { MappingDialog, type MappingDialogData } from './mapping-dialog';
import { SpDialog, type SpDialogData } from './sp-dialog';

/**
 * Drill-down page for one SAML identity provider. Rather than tabbing between
 * disconnected lists, every facet of the IdP — its service providers,
 * attributes, and attribute-to-role mappings — is a stacked section on one
 * page, so the page reads as "everything about this identity provider".
 *
 * SPs and attributes are naturally small per IdP (fetched whole and filtered
 * client-side); role mappings can grow large, so they are server-filtered and
 * paginated with an <odo-paginator> beneath the table.
 */
@Component({
  selector: 'app-idp-detail',
  imports: [
    CdkTableModule,
    FormsModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressSpinnerModule,
    DetailPage,
    OdoTable,
    OdoSortHeader,
    OdoPaginator,
  ],
  templateUrl: './idp-detail.html',
  styleUrl: './idp-detail.scss',
})
export class IdpDetail {
  /** Route param, bound via withComponentInputBinding. */
  readonly id = input.required({ transform: numberAttribute });

  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly spColumns = ['label', 'acs_url', 'status', 'actions'];
  protected readonly attrColumns = ['key', 'label', 'normalizer', 'flags', 'actions'];
  protected readonly mapColumns = ['attribute', 'value', 'role', 'status', 'actions'];

  protected readonly loading = signal(true);
  protected readonly idp = signal<IdpRow | null>(null);
  protected readonly canWrite = signal(false);

  // SPs and attributes are naturally small per IdP: fetch all, filter here.
  private readonly allSps = signal<SpRow[]>([]);
  private readonly allAttributes = signal<AttributeRow[]>([]);
  protected readonly roles = signal<RoleRow[]>([]);

  protected readonly sps = computed(() =>
    this.allSps().filter((sp) => sp.idp === this.id()),
  );
  protected readonly attributes = computed(() =>
    this.allAttributes().filter((a) => a.idp === this.id()),
  );

  // Mappings can grow large, so they're server-filtered and paginated.
  protected readonly mappings = signal<AttrRoleMapRow[]>([]);
  protected readonly mappingTotal = signal(0);
  protected readonly mappingsLoading = signal(false);
  protected mappingSearch = '';
  protected readonly mappingSort = signal<OdoSort>({
    active: 'role',
    direction: 'asc',
  });
  protected readonly pageSize = signal(25);
  protected readonly pageIndex = signal(0);

  constructor() {
    this.auth.hasPerm(PERMS.SAML_WRITE).then((ok) => this.canWrite.set(ok));

    // Reload whenever the route id changes. This component is reused across
    // same-route :id changes, so ngOnInit would only fire once — an effect
    // tracking id() reloads on every change, including the initial one. Only
    // id() is read synchronously here, so pagination/sort signals read later in
    // loadMappings don't become effect dependencies.
    effect(() => {
      const id = this.id();
      void this.reload(id);
    });
  }

  protected async reload(id: number = this.id()): Promise<void> {
    this.loading.set(true);
    try {
      const [idps, sps, attributes] = await Promise.all([
        samlAdminApi.listIdps(),
        samlAdminApi.listSps(),
        samlAdminApi.listAttributes(),
      ]);
      const idp = idps.find((i) => i.id === id);
      if (!idp) {
        this.errors.show(new ApiRequestError(404), 'Identity provider not found');
        void this.router.navigate(['/saml']);
        return;
      }
      this.idp.set(idp);
      this.allSps.set(sps);
      this.allAttributes.set(attributes);
    } catch (err) {
      this.errors.show(err, 'Failed to load identity provider');
    } finally {
      this.loading.set(false);
    }

    await this.loadMappings();

    // Role list for the mapping dialog; non-fatal when missing.
    try {
      this.roles.set(await authzAdminApi.listRoles());
    } catch (err) {
      console.error('Failed to load roles for mapping dialog:', err);
    }
  }

  /** Server-filtered, paginated fetch for the Role Mappings section. */
  protected async loadMappings(): Promise<void> {
    this.mappingsLoading.set(true);
    try {
      const result = await samlAdminApi.listAttrRoleMaps({
        idp: this.id(),
        search: this.mappingSearch.trim() || undefined,
        sort_by: this.mappingSort().active,
        sort_dir: this.mappingSort().direction,
        limit: this.pageSize(),
        offset: this.pageIndex() * this.pageSize(),
      });
      this.mappings.set(result.rows);
      this.mappingTotal.set(result.total);
    } catch (err) {
      this.errors.show(err, 'Failed to load role mappings');
    } finally {
      this.mappingsLoading.set(false);
    }
  }

  protected applyMappingSearch(): void {
    this.pageIndex.set(0);
    void this.loadMappings();
  }

  protected onMappingSort(sort: OdoSort): void {
    this.mappingSort.set(sort);
    this.pageIndex.set(0);
    void this.loadMappings();
  }

  protected onMappingPage(event: OdoPageEvent): void {
    this.pageIndex.set(event.pageIndex);
    this.pageSize.set(event.pageSize);
    void this.loadMappings();
  }

  // --- IdP ---

  protected editIdp(): void {
    const idp = this.idp();
    if (!idp) return;
    const data: IdpDialogData = { idp };
    const ref = this.dialog.open(IdpDialog, { data, width: '560px' });
    ref.afterClosed().subscribe((saved?: IdpRow) => {
      if (saved) this.idp.set(saved);
    });
  }

  protected async deleteIdp(): Promise<void> {
    const idp = this.idp();
    if (!idp) return;
    const confirmed = await this.confirm({
      title: $localize`Delete identity provider?`,
      message: $localize`"${idp.name}" will be permanently deleted.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await samlAdminApi.deleteIdp(idp.id);
      void this.router.navigate(['/saml']);
    } catch (err) {
      this.errors.show(err, 'Failed to delete identity provider');
    }
  }

  // --- Service providers ---

  protected openSpDialog(sp?: SpRow): void {
    const idp = this.idp();
    if (!idp) return;
    const data: SpDialogData = { sp, idps: [idp], defaultIdp: idp.id };
    const ref = this.dialog.open(SpDialog, { data, width: '560px' });
    ref.afterClosed().subscribe((saved?: SpRow) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteSp(sp: SpRow): Promise<void> {
    const confirmed = await this.confirm({
      title: $localize`Delete service provider?`,
      message: $localize`"${sp.label || sp.entity_id}" will be permanently deleted.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await samlAdminApi.deleteSp(sp.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete service provider');
    }
  }

  // --- Attributes ---

  protected openAttributeDialog(attribute?: AttributeRow): void {
    const idp = this.idp();
    if (!idp) return;
    const data: AttributeDialogData = { attribute, idps: [idp], defaultIdp: idp.id };
    const ref = this.dialog.open(AttributeDialog, { data, width: '520px' });
    ref.afterClosed().subscribe((saved?: AttributeRow) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteAttribute(attribute: AttributeRow): Promise<void> {
    const confirmed = await this.confirm({
      title: $localize`Delete SAML attribute?`,
      message: $localize`"${attribute.key}" will no longer be tracked.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await samlAdminApi.deleteAttribute(attribute.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete attribute');
    }
  }

  // --- Role mappings ---

  protected openMappingDialog(mapping?: AttrRoleMapRow): void {
    const data: MappingDialogData = {
      mapping,
      attributes: this.attributes(),
      roles: this.roles(),
    };
    const ref = this.dialog.open(MappingDialog, { data, width: '520px' });
    ref.afterClosed().subscribe((saved?: AttrRoleMapRow) => {
      if (saved) void this.reload();
    });
  }

  protected async deleteMapping(mapping: AttrRoleMapRow): Promise<void> {
    const confirmed = await this.confirm({
      title: $localize`Delete role mapping?`,
      message: $localize`"${mapping.attr_value}" will no longer grant ${mapping.role_label}.`,
      confirmLabel: $localize`Delete`,
    });
    if (!confirmed) return;

    try {
      await samlAdminApi.deleteAttrRoleMap(mapping.id);
      await this.reload();
    } catch (err) {
      this.errors.show(err, 'Failed to delete mapping');
    }
  }

  private confirm(data: ConfirmDialogData): Promise<boolean> {
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    return firstValueFrom(ref.afterClosed()).then((result) => result === true);
  }
}
