/**
 * Typed client for the odo-org admin endpoints. Request/response shapes are
 * the generated types from the committed OpenAPI spec (source of truth: the
 * Rust structs), so a backend field change surfaces here as a compile error.
 */
import { apiGet, apiPost } from '../../core/api';
import type { components } from '../../core/api-types/odo-org';

type S = components['schemas'];

export type OrgUnitRow = S['UnitRow'];
export type UnitType = S['UnitTypeRow'];
export type UnitTypePage = S['UnitTypePage'];
export type OrgAddress = S['AddressRow'];
export type OrgClosure = S['ClosureRow'];
export type OrgOperatingHours = S['OperatingHoursRow'];
export type OrgUnitChildren = S['OrgUnitChildrenResponse'];

/** Flattened tree row for the list view: hierarchical order + depth. */
export interface TreeUnit {
  id: number;
  label: string;
  code: string;
  parent: number | null;
  unit_type: { id: number; label: string };
  timezone: string | null;
  depth: number;
  has_children: boolean;
}

interface TreeNode {
  id: number;
  label: string;
  code: string;
  parent: number | null;
  unit_type: { id: number; label: string };
  timezone: string | null;
  children?: TreeNode[];
}

const BASE = '/api/v1/odo/org/admin';

export const orgAdminApi = {
  /**
   * Fetch the org tree fresh and flatten it into display order with depths.
   * (odo-org no longer caches; this is a live DB read, so admin edits show
   * immediately.)
   */
  async fetchTree(): Promise<TreeUnit[]> {
    const response = await apiGet<TreeNode | { tree: TreeNode }>(
      '/api/v1/odo/org/tree',
    );
    const root = 'tree' in response ? response.tree : response;

    const out: TreeUnit[] = [];
    const visit = (node: TreeNode, depth: number) => {
      const children = [...(node.children ?? [])].sort((a, b) =>
        a.label.localeCompare(b.label),
      );
      out.push({
        id: node.id,
        label: node.label,
        code: node.code,
        parent: node.parent,
        unit_type: node.unit_type,
        timezone: node.timezone,
        depth,
        has_children: children.length > 0,
      });
      for (const child of children) visit(child, depth + 1);
    };
    visit(root, 0);
    return out;
  },

  async listUnitTypes(search?: string): Promise<UnitType[]> {
    const result = await apiPost<UnitTypePage>(`${BASE}/unit-type/list`, {
      search: search || undefined,
    });
    return result.rows;
  },

  createUnit(params: S['CreateUnitRequest']): Promise<OrgUnitRow> {
    return apiPost(`${BASE}/unit/create`, params);
  },
  updateUnit(id: number, params: Omit<S['UpdateUnitRequest'], 'id'>): Promise<OrgUnitRow> {
    return apiPost(`${BASE}/unit/update`, { id, ...params });
  },
  deleteUnit(id: number): Promise<{ success: boolean }> {
    return apiPost(`${BASE}/unit/delete`, { id });
  },

  createUnitType(params: S['CreateUnitTypeRequest']): Promise<UnitType> {
    return apiPost(`${BASE}/unit-type/create`, params);
  },
  updateUnitType(
    id: number,
    params: Omit<S['UpdateUnitTypeRequest'], 'id'>,
  ): Promise<UnitType> {
    return apiPost(`${BASE}/unit-type/update`, { id, ...params });
  },
  deleteUnitType(id: number): Promise<{ success: boolean }> {
    return apiPost(`${BASE}/unit-type/delete`, { id });
  },

  unitChildren(orgUnit: number): Promise<OrgUnitChildren> {
    return apiPost(`${BASE}/unit-children`, { org_unit: orgUnit });
  },

  createAddress(orgUnit: number, params: Omit<S['CreateAddressRequest'], 'org_unit'>): Promise<OrgAddress> {
    return apiPost(`${BASE}/address/create`, { org_unit: orgUnit, ...params });
  },
  updateAddress(id: number, params: Omit<S['UpdateAddressRequest'], 'id'>): Promise<OrgAddress> {
    return apiPost(`${BASE}/address/update`, { id, ...params });
  },
  deleteAddress(id: number): Promise<{ success: boolean }> {
    return apiPost(`${BASE}/address/delete`, { id });
  },

  createClosure(orgUnit: number, params: Omit<S['CreateClosureRequest'], 'org_unit'>): Promise<OrgClosure> {
    return apiPost(`${BASE}/closure/create`, { org_unit: orgUnit, ...params });
  },
  updateClosure(id: number, params: Omit<S['UpdateClosureRequest'], 'id'>): Promise<OrgClosure> {
    return apiPost(`${BASE}/closure/update`, { id, ...params });
  },
  deleteClosure(id: number): Promise<{ success: boolean }> {
    return apiPost(`${BASE}/closure/delete`, { id });
  },

  createHours(orgUnit: number, params: Omit<S['CreateOperatingHoursRequest'], 'org_unit'>): Promise<OrgOperatingHours> {
    return apiPost(`${BASE}/operating-hours/create`, { org_unit: orgUnit, ...params });
  },
  updateHours(id: number, params: Omit<S['UpdateOperatingHoursRequest'], 'id'>): Promise<OrgOperatingHours> {
    return apiPost(`${BASE}/operating-hours/update`, { id, ...params });
  },
  deleteHours(id: number): Promise<{ success: boolean }> {
    return apiPost(`${BASE}/operating-hours/delete`, { id });
  },
};

/** Ids of `unit` and every unit beneath it (for parent-picker exclusion). */
export function subtreeIds(units: TreeUnit[], unitId: number): Set<number> {
  const childrenOf = new Map<number | null, TreeUnit[]>();
  for (const u of units) {
    const list = childrenOf.get(u.parent) ?? [];
    list.push(u);
    childrenOf.set(u.parent, list);
  }
  const ids = new Set<number>();
  const visit = (id: number) => {
    ids.add(id);
    for (const child of childrenOf.get(id) ?? []) visit(child.id);
  };
  visit(unitId);
  return ids;
}
