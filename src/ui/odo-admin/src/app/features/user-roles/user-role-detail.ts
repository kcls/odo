import {
  Component,
  effect,
  inject,
  input,
  numberAttribute,
  signal,
} from '@angular/core';
import { DatePipe } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { CdkTableModule } from '@angular/cdk/table';
import { MatButtonModule } from '@angular/material/button';
import { MatDialog } from '@angular/material/dialog';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressSpinnerModule } from '@angular/material/progress-spinner';
import { MatSelectModule } from '@angular/material/select';
import { MatSnackBar } from '@angular/material/snack-bar';
import { firstValueFrom } from 'rxjs';
import { authApi, orgUnitApi, type OrgUnit } from '@odo/core';

import { ApiRequestError } from '../../core/api';
import { ErrorHandlerService } from '../../core/error-handler.service';
import { DetailPage } from '../../shared/detail-page/detail-page';
import { OdoTable } from '../../shared/odo-table/odo-table';
import {
  ConfirmDialog,
  type ConfirmDialogData,
} from '../../shared/confirm-dialog';
import { authzAdminApi, type RoleRow } from '../roles/authz-api';
import {
  userRoleApi,
  type AssignmentRow,
  type PermScopeRow,
} from './user-role-api';
import type { UserRow } from './user-search';

interface OrgOption {
  id: number;
  label: string;
  depth: number;
}

/** Flatten the org tree (flat-with-parent or nested) into indented options. */
function flattenOrgTree(units: OrgUnit[]): OrgOption[] {
  const all = new Map<number, OrgUnit>();
  const collect = (list: OrgUnit[]) => {
    for (const u of list) {
      if (!all.has(u.id)) all.set(u.id, u);
      if (u.children?.length) collect(u.children);
    }
  };
  collect(units);

  const byParent = new Map<number | null, OrgUnit[]>();
  for (const u of all.values()) {
    if (u.deleted_at) continue;
    const key = u.parent ?? null;
    const list = byParent.get(key) ?? [];
    list.push(u);
    byParent.set(key, list);
  }

  const out: OrgOption[] = [];
  const visit = (parent: number | null, depth: number) => {
    const children = (byParent.get(parent) ?? []).sort((a, b) =>
      a.label.localeCompare(b.label),
    );
    for (const u of children) {
      out.push({ id: u.id, label: u.label, depth });
      visit(u.id, depth + 1);
    }
  };
  visit(null, 0);
  return out;
}

/**
 * Drill-down page for one user's role assignments: the user's grants at org
 * units, with an inline add form and per-row removal. SAML-managed assignments
 * are read-only (owned by the IdP mapping).
 */
@Component({
  selector: 'app-user-role-detail',
  imports: [
    DatePipe,
    FormsModule,
    CdkTableModule,
    MatButtonModule,
    MatFormFieldModule,
    MatIconModule,
    MatProgressSpinnerModule,
    MatSelectModule,
    DetailPage,
    OdoTable,
  ],
  templateUrl: './user-role-detail.html',
  styleUrl: './user-role-detail.scss',
})
export class UserRoleDetail {
  /** Route param, bound via withComponentInputBinding. */
  readonly id = input.required({ transform: numberAttribute });

  private readonly router = inject(Router);
  private readonly dialog = inject(MatDialog);
  private readonly snackBar = inject(MatSnackBar);
  private readonly errors = inject(ErrorHandlerService);

  protected readonly columns = ['role', 'org_unit', 'source', 'added', 'actions'];

  protected readonly loading = signal(true);
  protected readonly user = signal<UserRow | null>(null);
  protected readonly assignments = signal<AssignmentRow[]>([]);
  protected readonly roles = signal<RoleRow[]>([]);
  protected readonly orgOptions = signal<OrgOption[]>([]);

  // Effective permissions (read-only): what the user can do and where.
  protected readonly permColumns = ['perm', 'scope'];
  protected readonly permScopes = signal<PermScopeRow[]>([]);
  protected readonly scopesLoading = signal(true);
  // Perms whose full scope list is expanded (keyed by perm code).
  protected readonly expanded = signal<Set<string>>(new Set());

  // Add-assignment form state
  protected newRole = '';
  protected newOrgUnit: number | null = null;
  protected readonly adding = signal(false);
  protected readonly addError = signal('');

  constructor() {
    // Pickers (roles + org tree) are id-independent: load once.
    void this.loadPickers();

    // Reload user + assignments + effective scopes whenever the route id
    // changes. This component is reused across same-route :id changes, so
    // ngOnInit would only fire once — an effect tracking id() reloads on every
    // change, including the initial one.
    effect(() => {
      const id = this.id();
      void this.load(id);
      void this.loadPermScopes(id);
    });
  }

  private async loadPermScopes(id: number = this.id()): Promise<void> {
    this.scopesLoading.set(true);
    try {
      this.permScopes.set(await userRoleApi.permScopes(id));
    } catch (err) {
      this.errors.show(err, "Failed to load the user's effective permissions");
    } finally {
      this.scopesLoading.set(false);
    }
  }

  /** Whether a permission's full scope list is expanded. */
  protected isExpanded(perm: string): boolean {
    return this.expanded().has(perm);
  }

  protected toggleExpanded(perm: string): void {
    const next = new Set(this.expanded());
    if (!next.delete(perm)) next.add(perm);
    this.expanded.set(next);
  }

  private async load(id: number = this.id()): Promise<void> {
    this.loading.set(true);
    try {
      const [user, assignments] = await Promise.all([
        authApi.getUser({ id }) as Promise<UserRow>,
        userRoleApi.listAssignments(id),
      ]);
      this.user.set(user);
      this.assignments.set(assignments);
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 404) {
        this.snackBar.open(
          $localize`User not found.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
        void this.router.navigate(['/user-roles']);
        return;
      }
      this.errors.show(err, "Failed to load the user's role assignments");
    } finally {
      this.loading.set(false);
    }
  }

  private async reloadAssignments(): Promise<void> {
    this.assignments.set(await userRoleApi.listAssignments(this.id()));
    // Assignments changed, so effective scope may have too.
    void this.loadPermScopes();
  }

  private async loadPickers(): Promise<void> {
    try {
      const [roles, units] = await Promise.all([
        authzAdminApi.listRoles(),
        orgUnitApi.getOrgUnitTree(),
      ]);
      this.roles.set(roles);
      this.orgOptions.set(flattenOrgTree(units));
    } catch (err) {
      console.error('Failed to load role/org pickers:', err);
    }
  }

  protected async addAssignment(): Promise<void> {
    const user = this.user();
    if (!user || !this.newRole || this.newOrgUnit === null) return;

    this.adding.set(true);
    this.addError.set('');
    try {
      await userRoleApi.createAssignment({
        usr: user.id,
        role: this.newRole,
        org_unit: this.newOrgUnit,
      });
      this.newRole = '';
      this.newOrgUnit = null;
      await this.reloadAssignments();
    } catch (err) {
      if (err instanceof ApiRequestError && err.code === 'ALREADY_ASSIGNED') {
        this.addError.set(
          $localize`The user already has this role at this org unit.`,
        );
      } else if (err instanceof ApiRequestError && err.status === 403) {
        this.addError.set(
          $localize`You do not have permission to assign roles at that org unit.`,
        );
      } else {
        this.addError.set(
          err instanceof Error
            ? err.message
            : $localize`Failed to add the role assignment.`,
        );
      }
    } finally {
      this.adding.set(false);
    }
  }

  protected async removeAssignment(assignment: AssignmentRow): Promise<void> {
    const data: ConfirmDialogData = {
      title: $localize`Remove role assignment?`,
      message: $localize`${assignment.role_label} at ${assignment.org_unit_label} will be removed from this user.`,
      confirmLabel: $localize`Remove`,
    };
    const ref = this.dialog.open(ConfirmDialog, { data, width: '420px' });
    const confirmed = await firstValueFrom(ref.afterClosed());
    if (confirmed !== true) return;

    try {
      await userRoleApi.deleteAssignment(assignment.id);
      await this.reloadAssignments();
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 403) {
        this.snackBar.open(
          $localize`You do not have permission to remove roles at that org unit.`,
          $localize`:Snackbar dismiss action:Dismiss`,
        );
      } else {
        this.errors.show(err, 'Failed to remove role assignment');
      }
    }
  }
}
