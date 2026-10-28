import { describe, expect, it, vi } from "vitest"

import { fetchAllPages, MAX_PAGE_LIMIT } from "./paging"

describe("fetchAllPages", () => {
  it("reads every page at the server's cap", async () => {
    const all = Array.from({ length: 450 }, (_, i) => i)
    const fetchPage = vi.fn(async ({ limit, offset }: { limit: number; offset: number }) => ({
      rows: all.slice(offset, offset + limit),
      total: all.length,
    }))
    expect(await fetchAllPages(fetchPage)).toEqual(all)
    expect(fetchPage.mock.calls.map(([p]) => p)).toEqual([
      { limit: MAX_PAGE_LIMIT, offset: 0 },
      { limit: MAX_PAGE_LIMIT, offset: 200 },
      { limit: MAX_PAGE_LIMIT, offset: 400 },
    ])
  })

  it("stops on an empty page even if the total says otherwise", async () => {
    const fetchPage = vi.fn(async ({ offset }: { limit: number; offset: number }) => ({
      rows: offset === 0 ? [1, 2] : [],
      total: 5,
    }))
    expect(await fetchAllPages(fetchPage)).toEqual([1, 2])
    expect(fetchPage).toHaveBeenCalledTimes(2)
  })
})
