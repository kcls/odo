import { describe, expect, it } from "vitest"

import { weekdayName } from "./format"

describe("weekdayName", () => {
  it("maps 0 to Sunday through 6 to Saturday", () => {
    expect([0, 1, 6].map((d) => weekdayName(d, "en"))).toEqual(["Sunday", "Monday", "Saturday"])
  })
})
