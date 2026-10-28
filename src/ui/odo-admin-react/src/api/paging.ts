/** The server's per-request cap (odo_service::admin::MAX_PAGE_LIMIT). */
export const MAX_PAGE_LIMIT = 200

/**
 * Every row of a paginated admin list, for pickers. Admin lists default
 * to 50 rows per request, so a single unpaged call silently truncates.
 * Meant for reference data (roles, permissions), not growing lists.
 */
export async function fetchAllPages<T>(
  fetchPage: (page: { limit: number; offset: number }) => Promise<{ rows: T[]; total: number }>
): Promise<T[]> {
  const rows: T[] = []
  for (;;) {
    const page = await fetchPage({ limit: MAX_PAGE_LIMIT, offset: rows.length })
    rows.push(...page.rows)
    // An empty page guards against a total that shrank mid-read.
    if (page.rows.length === 0 || rows.length >= page.total) return rows
  }
}
