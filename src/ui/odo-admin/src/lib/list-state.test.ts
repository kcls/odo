import { describe, expect, it } from "vitest"

import {
  listRequest,
  readListState,
  redirectPastLastPage,
  searchTerm,
  writeListState,
  type ListDefaults,
} from "./list-state"

const LIST: ListDefaults = { sort: { key: "code", dir: "asc" }, sortKeys: ["code", "description"] }

describe("readListState", () => {
  it("uses defaults for an empty query", () => {
    expect(readListState(new URLSearchParams(), LIST)).toEqual({
      q: "",
      sort: { key: "code", dir: "asc" },
      pageIndex: 0,
      pageSize: 25,
    })
  })

  it("reads a full query with a one-based page", () => {
    const state = readListState(new URLSearchParams("q=circ&sort=description&dir=desc&page=3&size=50"), LIST)
    expect(state).toEqual({ q: "circ", sort: { key: "description", dir: "desc" }, pageIndex: 2, pageSize: 50 })
  })

  it("ignores sort keys outside the allow-list", () => {
    expect(readListState(new URLSearchParams("sort=password&dir=desc"), LIST).sort).toEqual(LIST.sort)
  })

  it("ignores bad page numbers and sizes", () => {
    const state = readListState(new URLSearchParams("page=-4&size=7"), LIST)
    expect(state.pageIndex).toBe(0)
    expect(state.pageSize).toBe(25)
  })
})

describe("writeListState", () => {
  it("leaves defaults out of the URL and keeps unrelated params", () => {
    const params = writeListState(
      new URLSearchParams("tab=x&q=old&page=4"),
      { q: "", sort: LIST.sort, pageIndex: 0, pageSize: 25 },
      LIST
    )
    expect(params.toString()).toBe("tab=x")
  })

  it("round-trips through readListState", () => {
    const state = { q: "a b", sort: { key: "description", dir: "desc" as const }, pageIndex: 1, pageSize: 100 }
    expect(readListState(writeListState(new URLSearchParams(), state, LIST), LIST)).toEqual(state)
  })
})

describe("listRequest", () => {
  it("maps to limit/offset and omits a blank search", () => {
    expect(listRequest({ q: "  ", sort: LIST.sort, pageIndex: 2, pageSize: 10 })).toEqual({
      search: undefined,
      sort_by: "code",
      sort_dir: "asc",
      limit: 10,
      offset: 20,
    })
  })
})

describe("redirectPastLastPage", () => {
  const url = new URL("http://x/permissions?q=a&page=5")
  const state = readListState(url.searchParams, LIST)

  /** The redirect's Location, or null when it didn't redirect. */
  const redirectFor = (total: number) => {
    try {
      redirectPastLastPage(url, state, total, LIST)
      return null
    } catch (e) {
      if (!(e instanceof Response)) throw e
      return e.headers.get("Location")
    }
  }

  it("redirects to the new last page, keeping the query", () => {
    expect(redirectFor(30)).toBe("/permissions?q=a&page=2")
  })

  it("drops the page param when nothing is left", () => {
    expect(redirectFor(0)).toBe("/permissions?q=a")
  })

  it("does nothing on a page that exists", () => {
    expect(redirectFor(101)).toBeNull()
  })
})

describe("searchTerm", () => {
  it("sends three or more characters, trimmed", () => {
    expect(searchTerm("  circ ")).toBe("circ")
    expect(searchTerm("abc")).toBe("abc")
  })

  it("sends nothing for shorter text", () => {
    expect(searchTerm("")).toBeUndefined()
    expect(searchTerm("ab")).toBeUndefined()
    expect(searchTerm("  ab  ")).toBeUndefined()
  })

  it("keeps a short query out of the list request", () => {
    const state = readListState(new URLSearchParams("q=ci"), LIST)
    expect(listRequest(state).search).toBeUndefined()
  })
})
