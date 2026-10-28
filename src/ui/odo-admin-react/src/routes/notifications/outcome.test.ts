import { describe, expect, it } from "vitest"

import { outcomeOf, outcomeTotals, statusesFor } from "./outcome"

describe("outcomes", () => {
  it("folds pending and processing into queued", () => {
    expect(outcomeOf("pending")).toBe("queued")
    expect(outcomeOf("processing")).toBe("queued")
    expect(statusesFor("queued")).toEqual(["pending", "processing"])
    expect(statusesFor("failed")).toEqual(["failed"])
  })

  it("ignores statuses it doesn't know", () => {
    expect(outcomeOf("bounced")).toBeNull()
  })

  it("totals summary rows across channels", () => {
    expect(
      outcomeTotals([
        { status: "delivered", count: 5 },
        { status: "pending", count: 2 },
        { status: "processing", count: 1 },
        { status: "delivered", count: 3 },
        { status: "mystery", count: 9 },
      ])
    ).toEqual({ delivered: 8, failed: 0, skipped: 0, queued: 3 })
  })
})
