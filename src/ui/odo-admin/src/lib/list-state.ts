import { redirect, useSearchParams } from "react-router"

import { appPath } from "@/lib/loader"

/**
 * Server-driven list state (search, sort, page) lives in the URL so a
 * list view deep-links and survives reloads and back/forward. Loaders
 * read it with readListState(); components update it with useListState().
 *
 *   ?q=circ&sort=description&dir=desc&page=2&size=50
 *
 * Defaults are left out of the URL to keep links short.
 */

export type SortDir = "asc" | "desc"
export type SortState = { key: string; dir: SortDir }

export type ListState = {
  q: string
  sort: SortState
  /** Zero-based. The URL's `page` is one-based for people. */
  pageIndex: number
  pageSize: number
}

export const PAGE_SIZES = [10, 25, 50, 100] as const
const DEFAULT_PAGE_SIZE = 25

export type ListDefaults = { sort: SortState; sortKeys: readonly string[] }

export function readListState(params: URLSearchParams, defaults: ListDefaults): ListState {
  const key = params.get("sort")
  const sort: SortState =
    key && defaults.sortKeys.includes(key)
      ? { key, dir: params.get("dir") === "desc" ? "desc" : "asc" }
      : defaults.sort
  const page = Number.parseInt(params.get("page") ?? "", 10)
  const size = Number.parseInt(params.get("size") ?? "", 10)
  return {
    q: params.get("q") ?? "",
    sort,
    pageIndex: Number.isFinite(page) && page > 1 ? page - 1 : 0,
    pageSize: (PAGE_SIZES as readonly number[]).includes(size) ? size : DEFAULT_PAGE_SIZE,
  }
}

/** The odo admin list request fields (Page + Sort + search) for a state. */
export function listRequest(state: ListState) {
  return {
    search: state.q.trim() || undefined,
    sort_by: state.sort.key,
    sort_dir: state.sort.dir,
    limit: state.pageSize,
    offset: state.pageIndex * state.pageSize,
  }
}

export function writeListState(
  params: URLSearchParams,
  next: ListState,
  defaults: ListDefaults
): URLSearchParams {
  const out = new URLSearchParams(params)
  const set = (name: string, value: string, isDefault: boolean) => {
    if (isDefault) out.delete(name)
    else out.set(name, value)
  }
  const defaultSort = next.sort.key === defaults.sort.key && next.sort.dir === defaults.sort.dir
  set("q", next.q, next.q === "")
  set("sort", next.sort.key, defaultSort)
  set("dir", next.sort.dir, defaultSort)
  set("page", String(next.pageIndex + 1), next.pageIndex === 0)
  set("size", String(next.pageSize), next.pageSize === DEFAULT_PAGE_SIZE)
  return out
}

/**
 * For list loaders: past the last page (e.g. the last row of the last
 * page was just deleted), redirect to the new last page rather than show
 * an empty one.
 */
export function redirectPastLastPage(url: URL, state: ListState, total: number, defaults: ListDefaults) {
  const last = Math.max(0, Math.ceil(total / state.pageSize) - 1)
  if (state.pageIndex > last) {
    const params = writeListState(url.searchParams, { ...state, pageIndex: last }, defaults)
    const query = params.toString()
    throw redirect(query ? `${appPath(url)}?${query}` : appPath(url))
  }
}

/**
 * The current list state plus an updater. Changing the search, sort, or
 * page size returns to the first page, since the old offset would skip
 * or repeat rows.
 */
export function useListState(defaults: ListDefaults) {
  const [params, setParams] = useSearchParams()
  const state = readListState(params, defaults)
  const update = (patch: Partial<ListState>) => {
    const resetsPage = patch.q !== undefined || patch.sort !== undefined || patch.pageSize !== undefined
    const next = { ...state, ...(resetsPage && patch.pageIndex === undefined ? { pageIndex: 0 } : {}), ...patch }
    setParams((prev) => writeListState(prev, next, defaults), { replace: patch.q !== undefined })
  }
  return [state, update] as const
}
