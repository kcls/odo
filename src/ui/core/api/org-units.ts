// Org Unit API client - core utility for all UI applications
// Communicates with odo-org via HTTP/JSON
import type { RoleAssignment } from '../types';
import { getApiBaseUrl } from '../utils/api-config';
import { ensureTokenFresh } from '../utils/token-refresh';
import { authApi } from './auth';

export interface OrgUnitType {
    id: number;
    label: string;
    parent: number | null;
    can_have_staff?: boolean;
    can_have_patrons?: boolean;
}

export interface OrgUnit {
  id: number;
  parent: number | null;
  label: string;
  code?: string;
  display_label?: string;
  description?: string | null;
  unit_type: OrgUnitType;
  unit_type_label?: string;
  timezone?: string;
  created_at?: string;
  updated_at?: string;
  deleted_at?: string | null;
  children?: OrgUnit[];
}

export interface OrgAddress {
  id: number;
  org_unit: number;
  address_type: string;
  label: string;
  address_line1?: string;
  address_line2?: string;
  city?: string;
  state_province?: string;
  postal_code?: string;
  deleted_at?: string | null;
}

export interface OrgClosure {
  id: number;
  org_unit: number;
  closure_date: string;
  reason: string;
  is_emergency: boolean;
  created_by?: number;
  created_at?: string;
}

export interface OrgUnitTreeResponse {
  tree: OrgUnit[];
}

export interface OrgUnitOperatingHours {
  id: number;
  org_unit: number;
  day_of_week: number;
  open_time: string;
  close_time: string;
  is_closed: boolean;
}

interface RawOrgUnitNode {
  id: number;
  parent?: number | null;
  label: string;
  code?: string;
  description?: string | null;
  unit_type: OrgUnitType;
  timezone?: string;
  created_at?: string;
  updated_at?: string;
  deleted_at?: string | null;
  children?: RawOrgUnitNode[];
}

interface OrgUnitDetailResponse {
  org_unit: RawOrgUnitNode;
  addresses: OrgAddress[];
  operating_hours: Array<Omit<OrgUnitOperatingHours, 'org_unit'>>;
  future_closures: Array<Omit<OrgClosure, 'org_unit'>>;
}

export interface OrgUnitDetail {
  org_unit: OrgUnit;
  addresses: OrgAddress[];
  operating_hours: OrgUnitOperatingHours[];
  future_closures: OrgClosure[];
}

async function orgGet<T>(path: string): Promise<T> {
  await ensureTokenFresh();
  const token = authApi.getToken();

  const response = await fetch(`${getApiBaseUrl()}/api/v1/odo/org${path}`, {
    headers: {
      ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
    },
  });

  if (!response.ok) {
    if (response.status === 401) authApi.sessionExpired$.next();
    throw new Error(`Org API error: ${response.status}`);
  }

  return response.json();
}

export class OrgUnitApi {
  private orgUnitTreeCache: OrgUnit[] | null = null;
  private orgUnitTreePromise: Promise<OrgUnit[]> | null = null;
  private orgUnitDetailCache = new Map<number, OrgUnitDetail>();
  private orgUnitDetailPromises = new Map<number, Promise<OrgUnitDetail>>();

  async getOrgUnitTree(): Promise<OrgUnit[]> {
    if (this.orgUnitTreeCache) {
      return this.orgUnitTreeCache;
    }

    const promise = this.orgUnitTreePromise ?? (
      this.orgUnitTreePromise = this.loadOrgUnitTree()
        .then(tree => {
          this.orgUnitTreeCache = tree;
          this.orgUnitTreePromise = null;
          return tree;
        })
        .catch(error => {
          this.orgUnitTreePromise = null;
          throw error;
        })
    );

    return promise;
  }

  private async loadOrgUnitTree(): Promise<OrgUnit[]> {
    try {
      const response = await orgGet<RawOrgUnitNode>('/tree');

      const rawTree = (response as any).tree ?? response;

      const nodes: RawOrgUnitNode[] = Array.isArray(rawTree)
        ? rawTree
        : [rawTree];

      const transformed = nodes.map(node => this.transformOrgUnitNode(node));
      this.sortOrgUnits(transformed);
      return transformed;
    } catch (error) {
      throw Error('Org unit retrieval failed: ' + error);
    }
  }

  private transformOrgUnitNode(node: RawOrgUnitNode): OrgUnit {
    const children = node.children?.map(child => this.transformOrgUnitNode(child));

    const orgUnit: OrgUnit = {
      id: Number(node.id),
      parent: node.parent ?? null,
      label: node.label,
      code: node.code,
      display_label:
        node.code && node.label ? `${node.code} / ${node.label}` : node.label,
      description: node.description ?? null,
      unit_type: node.unit_type,
      unit_type_label: node.unit_type?.label,
      timezone: node.timezone,
      created_at: node.created_at,
      updated_at: node.updated_at,
      deleted_at: node.deleted_at ?? null,
      children,
    };

    return orgUnit;
  }

  private sortOrgUnits(units: OrgUnit[]) {
    units.sort((a, b) => a.label.localeCompare(b.label));
    for (const unit of units) {
      if (unit.children && unit.children.length > 0) {
        this.sortOrgUnits(unit.children);
      }
    }
  }

  flattenOrgUnits(units: OrgUnit[], level = 0): Array<OrgUnit & { level: number }> {
    const flattened: Array<OrgUnit & { level: number }> = [];

    for (const unit of units) {
      flattened.push({ ...unit, level });
      if (unit.children && unit.children.length > 0) {
        flattened.push(...this.flattenOrgUnits(unit.children, level + 1));
      }
    }

    return flattened;
  }

  findUnitById(units: OrgUnit[], id: number): OrgUnit | null {
    for (const unit of units) {
      if (unit.id === id) {
        return unit;
      }
      if (unit.children && unit.children.length > 0) {
        const found = this.findUnitById(unit.children, id);
        if (found) return found;
      }
    }
    return null;
  }

  getUnitDisplayName(unit: OrgUnit & { level?: number }): string {
    if (unit.level && unit.level > 0) {
      return `${'  '.repeat(unit.level)}${unit.label}`;
    }
    return unit.label;
  }

  async getOrgUnitDescendants(targetId: number): Promise<number[]> {
    try {
      const response = await orgGet<RawOrgUnitNode[]>(`/unit/${targetId}/descendants`);

      const descendants = Array.isArray(response) ? response : [response];
      return descendants.map((unit: RawOrgUnitNode) => Number(unit.id));
    } catch (error) {
      throw Error('Failed to get org unit descendants: ' + error);
    }
  }

  async getOrgUnitAncestors(targetId: number): Promise<OrgUnit[]> {
    try {
      const response = await orgGet<RawOrgUnitNode[]>(`/unit/${targetId}/ancestors`);

      const ancestors = Array.isArray(response) ? response : [response];
      return ancestors.map((node: RawOrgUnitNode) => this.transformOrgUnitNode(node));
    } catch (error) {
      throw Error('Failed to get org unit ancestors: ' + error);
    }
  }

  async getAccessibleOrgUnits<T extends { id: number; parent: number | null }>(
    allOrgUnits: T[],
    userRoles: RoleAssignment[] | undefined,
    roles: string[]
  ): Promise<T[]> {
    if (!userRoles || userRoles.length === 0) return [];

    const roleOrgUnits = userRoles
      .filter(r => roles.includes(r.role))
      .map(r => r.org_unit);

    if (roleOrgUnits.length === 0) return [];

    const descendantPromises = roleOrgUnits.map(orgUnitId =>
      this.getOrgUnitDescendants(orgUnitId)
    );
    const descendantResults = await Promise.all(descendantPromises);

    const orgUnitIds = new Set<number>();
    descendantResults.forEach(descendants => {
      descendants.forEach((id: number) => orgUnitIds.add(id));
    });

    return allOrgUnits.filter(unit => orgUnitIds.has(unit.id));
  }

  async getOrgUnitDetail(orgUnitId: number): Promise<OrgUnitDetail> {
    if (this.orgUnitDetailCache.has(orgUnitId)) {
      return this.orgUnitDetailCache.get(orgUnitId)!;
    }

    if (this.orgUnitDetailPromises.has(orgUnitId)) {
      return this.orgUnitDetailPromises.get(orgUnitId)!;
    }

    const promise = this.loadOrgUnitDetail(orgUnitId)
      .then(detail => {
        this.orgUnitDetailCache.set(orgUnitId, detail);
        this.orgUnitDetailPromises.delete(orgUnitId);
        return detail;
      })
      .catch(error => {
        this.orgUnitDetailPromises.delete(orgUnitId);
        throw error;
      });

    this.orgUnitDetailPromises.set(orgUnitId, promise);
    return promise;
  }

  private async loadOrgUnitDetail(orgUnitId: number): Promise<OrgUnitDetail> {
    const response = await orgGet<OrgUnitDetailResponse>(`/unit/${orgUnitId}`);

    const detail = response;
    const orgUnit = this.transformOrgUnitNode(detail.org_unit);

    const addresses: OrgAddress[] = (detail.addresses ?? []).map(addr => ({
      ...addr,
      org_unit: orgUnitId,
      deleted_at: addr.deleted_at ?? null,
    }));

    const operatingHours: OrgUnitOperatingHours[] =
      (detail.operating_hours ?? []).map(hours => ({
        ...hours,
        org_unit: orgUnitId,
      }));

    const futureClosures: OrgClosure[] =
      (detail.future_closures ?? []).map(closure => ({
        ...closure,
        org_unit: orgUnitId,
      }));

    return {
      org_unit: orgUnit,
      addresses,
      operating_hours: operatingHours,
      future_closures: futureClosures,
    };
  }

  async getOrgUnitAddresses(orgUnitId: number): Promise<OrgAddress[]> {
    const detail = await this.getOrgUnitDetail(orgUnitId);
    return detail.addresses;
  }

  async getOrgUnitOperatingHours(
    orgUnitId: number,
  ): Promise<OrgUnitOperatingHours[]> {
    const detail = await this.getOrgUnitDetail(orgUnitId);
    return detail.operating_hours;
  }

  async getOrgUnitFutureClosures(orgUnitId: number): Promise<OrgClosure[]> {
    const detail = await this.getOrgUnitDetail(orgUnitId);
    return detail.future_closures;
  }
}

export const orgUnitApi = new OrgUnitApi();
