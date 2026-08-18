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
import { samlAdminApi, type IdpRow } from './saml-api';
import { IdpDialog, type IdpDialogData } from './idp-dialog';

/**
 * SAML identity provider list. An IdP is the subject the user drills into, so
 * it is the primary framed table with clickable rows; its service providers,
 * attributes, and role mappings are managed on the IdP detail page.
 */
@Component({
  selector: 'app-idp-list',
  imports: [
    CdkTableModule,
    MatButtonModule,
    MatIconModule,
    MatProgressSpinnerModule,
    OdoTable,
  ],
  templateUrl: './idp-list.html',
})
export class IdpList implements OnInit {
  private readonly auth = inject(AuthService);
  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['name', 'entity_id', 'status', 'usage'];

  protected readonly loading = signal(true);
  protected readonly idps = signal<IdpRow[]>([]);
  protected readonly canWrite = signal(false);

  constructor() {
    this.auth.hasPerm(PERMS.SAML_WRITE).then((ok) => this.canWrite.set(ok));
  }

  ngOnInit(): void {
    void this.reload();
  }

  private async reload(): Promise<void> {
    this.loading.set(true);
    try {
      this.idps.set(await samlAdminApi.listIdps());
    } catch (err) {
      this.errors.show(err, 'Failed to load identity providers');
    } finally {
      this.loading.set(false);
    }
  }

  protected open(idp: IdpRow): void {
    void this.router.navigate(['/saml', idp.id]);
  }

  protected createIdp(): void {
    const data: IdpDialogData = {};
    const ref = this.dialog.open(IdpDialog, { data, width: '560px' });
    ref.afterClosed().subscribe((created?: IdpRow) => {
      if (created) void this.router.navigate(['/saml', created.id]);
    });
  }
}
