import { orgUnitApi } from "@odo/core"

import { apiGet, apiPost } from "./client"
import { fetchAllPages } from "./paging"
import type { components } from "./types/odo-org"

type S = components["schemas"]

export type UnitType = S["UnitTypeRow"]
export type OrgUnitNode = S["OrgUnitResponse"]
export type UnitRow = S["UnitRow"]
export type OrgAddress = S["AddressRow"]
export type OrgClosure = S["ClosureRow"]
export type OrgOperatingHours = S["OperatingHoursRow"]

/** The fields tree flattening needs; both odo-org and @odo/core trees fit. */
type TreeNode = {
  id: number
  parent?: number | null
  label: string
  deleted_at?: string | null
  children?: TreeNode[] | null
}

export type Flattened<T> = T & { depth: number; childCount: number }

/**
 * Flatten an org tree (nested, flat-with-parent, or both) into tree
 * order with depths, siblings sorted by label. Deleted units are dropped
 * along with their subtrees.
 */
export function flattenTree<T extends TreeNode>(roots: readonly T[]): Flattened<T>[] {
  const all = new Map<number, T>()
  const collect = (list: readonly T[]) => {
    for (const u of list) {
      if (!all.has(u.id)) all.set(u.id, u)
      if (u.children?.length) collect(u.children as T[])
    }
  }
  collect(roots)

  const byParent = new Map<number | null, T[]>()
  for (const u of all.values()) {
    if (u.deleted_at) continue
    const key = u.parent ?? null
    byParent.set(key, [...(byParent.get(key) ?? []), u])
  }

  const out: Flattened<T>[] = []
  const visit = (parent: number | null, depth: number) => {
    const children = (byParent.get(parent) ?? []).sort((a, b) => a.label.localeCompare(b.label))
    for (const u of children) {
      out.push({ ...u, depth, childCount: byParent.get(u.id)?.length ?? 0 })
      visit(u.id, depth + 1)
    }
  }
  visit(null, 0)
  return out
}

/**
 * Rows matching `query` (label or code, case-insensitive) plus their
 * ancestors, so each match keeps its place in the tree. An empty query
 * keeps everything.
 */
export function filterTree<T extends { id: number; parent?: number | null; label: string; code?: string }>(
  rows: readonly T[],
  query: string
): T[] {
  const q = query.trim().toLowerCase()
  if (!q) return [...rows]
  const byId = new Map(rows.map((r) => [r.id, r]))
  const keep = new Set<number>()
  for (const r of rows) {
    if (!r.label.toLowerCase().includes(q) && !r.code?.toLowerCase().includes(q)) continue
    for (let at: T | undefined = r; at && !keep.has(at.id); at = byId.get(at.parent ?? -1)) {
      keep.add(at.id)
    }
  }
  return rows.filter((r) => keep.has(r.id))
}

/** Ids of a unit and everything beneath it, from flattened rows. */
export function subtreeIds(rows: readonly { id: number; parent?: number | null }[], rootId: number): Set<number> {
  const ids = new Set([rootId])
  // Tree order puts every descendant after its ancestor.
  for (const r of rows) if (r.parent != null && ids.has(r.parent)) ids.add(r.id)
  return ids
}

export type OrgOption = { id: number; label: string; depth: number }

export function flattenOrgTree(units: readonly TreeNode[]): OrgOption[] {
  return flattenTree(units).map(({ id, label, depth }) => ({ id, label, depth }))
}

/** The org tree as picker options. Cached by @odo/core per session. */
export async function orgOptions(): Promise<OrgOption[]> {
  return flattenOrgTree(await orgUnitApi.getOrgUnitTree())
}

const ORG_ADMIN = "/api/v1/odo/org/admin"

/** Typed client for the odo-org admin endpoints. */
export const orgAdminApi = {
  /** The whole tree, read fresh (unlike @odo/core's cached copy). */
  async fetchTree(): Promise<Flattened<OrgUnitNode>[]> {
    return flattenTree([await apiGet<OrgUnitNode>("/api/v1/odo/org/tree")])
  },

  createUnit(params: S["CreateUnitRequest"]): Promise<UnitRow> {
    return apiPost(`${ORG_ADMIN}/unit/create`, params)
  },
  /** Omitted fields are unchanged; an empty timezone clears it. */
  updateUnit(id: number, params: Omit<S["UpdateUnitRequest"], "id">): Promise<UnitRow> {
    return apiPost(`${ORG_ADMIN}/unit/update`, { id, ...params })
  },
  deleteUnit(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/unit/delete`, { id })
  },

  /** A unit's addresses, closures, and operating hours. */
  unitChildren(orgUnit: number): Promise<S["OrgUnitChildrenResponse"]> {
    return apiPost(`${ORG_ADMIN}/unit-children`, { org_unit: orgUnit })
  },

  createAddress(params: S["CreateAddressRequest"]): Promise<OrgAddress> {
    return apiPost(`${ORG_ADMIN}/address/create`, params)
  },
  updateAddress(id: number, params: Omit<S["UpdateAddressRequest"], "id">): Promise<OrgAddress> {
    return apiPost(`${ORG_ADMIN}/address/update`, { id, ...params })
  },
  deleteAddress(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/address/delete`, { id })
  },

  createClosure(params: S["CreateClosureRequest"]): Promise<OrgClosure> {
    return apiPost(`${ORG_ADMIN}/closure/create`, params)
  },
  updateClosure(id: number, params: Omit<S["UpdateClosureRequest"], "id">): Promise<OrgClosure> {
    return apiPost(`${ORG_ADMIN}/closure/update`, { id, ...params })
  },
  deleteClosure(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/closure/delete`, { id })
  },

  createHours(params: S["CreateOperatingHoursRequest"]): Promise<OrgOperatingHours> {
    return apiPost(`${ORG_ADMIN}/operating-hours/create`, params)
  },
  updateHours(id: number, params: Omit<S["UpdateOperatingHoursRequest"], "id">): Promise<OrgOperatingHours> {
    return apiPost(`${ORG_ADMIN}/operating-hours/update`, { id, ...params })
  },
  deleteHours(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/operating-hours/delete`, { id })
  },

  /** Every unit type: reference data for the tree, small and bounded. */
  listAllUnitTypes(): Promise<UnitType[]> {
    return fetchAllPages((page) =>
      apiPost<S["UnitTypePage"]>(`${ORG_ADMIN}/unit-type/list`, { ...page, sort_by: "label" })
    )
  },
  createUnitType(params: S["CreateUnitTypeRequest"]): Promise<UnitType> {
    return apiPost(`${ORG_ADMIN}/unit-type/create`, params)
  },
  /** A null or omitted `parent` leaves it unchanged; it cannot be cleared. */
  updateUnitType(id: number, params: Omit<S["UpdateUnitTypeRequest"], "id">): Promise<UnitType> {
    return apiPost(`${ORG_ADMIN}/unit-type/update`, { id, ...params })
  },
  deleteUnitType(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/unit-type/delete`, { id })
  },
}
