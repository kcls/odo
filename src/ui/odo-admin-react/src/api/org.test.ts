import type { OrgUnit } from "@odo/core"
import { describe, expect, it } from "vitest"

import { flattenOrgTree } from "./org"

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
