import { orgUnitApi, type OrgUnit } from "@odo/core"

import { apiPost } from "./client"
import { fetchAllPages } from "./paging"
import type { components } from "./types/odo-org"

export type OrgOption = { id: number; label: string; depth: number }

/**
 * Flatten the org tree (flat-with-parent or nested) into depth-annotated
 * options in tree order, siblings sorted by label. Deleted units are
 * dropped along with their subtrees.
 */
export function flattenOrgTree(units: OrgUnit[]): OrgOption[] {
  const all = new Map<number, OrgUnit>()
  const collect = (list: OrgUnit[]) => {
    for (const u of list) {
      if (!all.has(u.id)) all.set(u.id, u)
      if (u.children?.length) collect(u.children)
    }
  }
  collect(units)

  const byParent = new Map<number | null, OrgUnit[]>()
  for (const u of all.values()) {
    if (u.deleted_at) continue
    const key = u.parent ?? null
    byParent.set(key, [...(byParent.get(key) ?? []), u])
  }

  const out: OrgOption[] = []
  const visit = (parent: number | null, depth: number) => {
    const children = (byParent.get(parent) ?? []).sort((a, b) => a.label.localeCompare(b.label))
    for (const u of children) {
      out.push({ id: u.id, label: u.label, depth })
      visit(u.id, depth + 1)
    }
  }
  visit(null, 0)
  return out
}

/** The org tree as picker options. Cached by @odo/core per session. */
export async function orgOptions(): Promise<OrgOption[]> {
  return flattenOrgTree(await orgUnitApi.getOrgUnitTree())
}

type OrgSchemas = components["schemas"]

export type UnitType = OrgSchemas["UnitTypeRow"]

const ORG_ADMIN = "/api/v1/odo/org/admin"

/** Typed client for the odo-org admin endpoints. */
export const orgAdminApi = {
  /** Every unit type: reference data for the tree, small and bounded. */
  listAllUnitTypes(): Promise<UnitType[]> {
    return fetchAllPages((page) =>
      apiPost<OrgSchemas["UnitTypePage"]>(`${ORG_ADMIN}/unit-type/list`, { ...page, sort_by: "label" })
    )
  },
  createUnitType(params: OrgSchemas["CreateUnitTypeRequest"]): Promise<UnitType> {
    return apiPost(`${ORG_ADMIN}/unit-type/create`, params)
  },
  /** A null or omitted `parent` leaves it unchanged; it cannot be cleared. */
  updateUnitType(id: number, params: Omit<OrgSchemas["UpdateUnitTypeRequest"], "id">): Promise<UnitType> {
    return apiPost(`${ORG_ADMIN}/unit-type/update`, { id, ...params })
  },
  deleteUnitType(id: number): Promise<{ success: boolean }> {
    return apiPost(`${ORG_ADMIN}/unit-type/delete`, { id })
  },
}
