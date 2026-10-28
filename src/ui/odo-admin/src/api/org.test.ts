import type { OrgUnit } from "@odo/core"
import { describe, expect, it } from "vitest"

import { filterTree, flattenOrgTree, flattenTree, subtreeIds } from "./org"

const unit = (id: number, parent: number | null, label: string, extra: Partial<OrgUnit> = {}): OrgUnit => ({
  id,
  parent,
  label,
  unit_type: { id: 1, label: "Branch", parent: null },
  ...extra,
})

describe("flattenOrgTree", () => {
  it("orders by tree, siblings by label, with depth", () => {
    const tree = [unit(1, null, "Root"), unit(3, 1, "Zeta"), unit(2, 1, "Alpha"), unit(4, 2, "Alpha child")]
    expect(flattenOrgTree(tree)).toEqual([
      { id: 1, label: "Root", depth: 0 },
      { id: 2, label: "Alpha", depth: 1 },
      { id: 4, label: "Alpha child", depth: 2 },
      { id: 3, label: "Zeta", depth: 1 },
    ])
  })

  it("accepts nested children and drops deleted subtrees", () => {
    const tree = [
      unit(1, null, "Root", {
        children: [unit(2, 1, "Gone", { deleted_at: "2026-01-01T00:00:00Z", children: [unit(5, 2, "Orphan")] }), unit(3, 1, "Kept")],
      }),
    ]
    expect(flattenOrgTree(tree).map((o) => o.label)).toEqual(["Root", "Kept"])
  })
})

describe("flattenTree", () => {
  it("keeps node fields and counts live children", () => {
    const tree = [unit(1, null, "Root"), unit(2, 1, "A"), unit(3, 1, "B", { deleted_at: "2026-01-01T00:00:00Z" })]
    expect(flattenTree(tree).map(({ id, depth, childCount, unit_type }) => ({ id, depth, childCount, type: unit_type.label }))).toEqual([
      { id: 1, depth: 0, childCount: 1, type: "Branch" },
      { id: 2, depth: 1, childCount: 0, type: "Branch" },
    ])
  })
})

describe("filterTree", () => {
  const rows = [
    { id: 1, parent: null, label: "Root", code: "ROOT" },
    { id: 2, parent: 1, label: "North Region", code: "N" },
    { id: 3, parent: 2, label: "Bellevue", code: "BEL" },
    { id: 4, parent: 1, label: "South Region", code: "S" },
  ]

  it("keeps matches and their ancestors, in order", () => {
    expect(filterTree(rows, "bell").map((r) => r.id)).toEqual([1, 2, 3])
  })

  it("matches codes", () => {
    expect(filterTree(rows, "s").map((r) => r.id)).toEqual([1, 4])
  })

  it("keeps everything for a blank query", () => {
    expect(filterTree(rows, "  ")).toHaveLength(4)
  })
})

describe("subtreeIds", () => {
  it("collects a unit and its descendants", () => {
    const rows = [
      { id: 1, parent: null },
      { id: 2, parent: 1 },
      { id: 3, parent: 2 },
      { id: 4, parent: 1 },
    ]
    expect([...subtreeIds(rows, 2)].sort()).toEqual([2, 3])
  })
})
