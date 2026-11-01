# Adding an admin tool

Every tool is built the same way, from the same parts, so adding one is
mostly a registry entry, a couple of route modules, and strings. Read one
existing tool end to end first: **`routes/roles/`** is the canonical
example (a server-driven list, a detail page, dialogs, a nested list of
grants), and **`routes/permissions/`** is the simplest list.

The rules that keep this app maintainable:

1. **Generated types are the source of truth.** Never hand-write a type
   that mirrors a backend struct; import from `src/api/types/<service>`.
2. **URLs are the state.** Search, sort, page, filters, and tabs live in
   the query string, so every view deep-links and survives a reload.
3. **Loaders load, middleware guards.** A route's `loader` fetches what it
   renders; the router's middleware has already checked the session and
   the tool's read permission, so a loader never makes a call the user
   isn't allowed to make.
4. **Lists are `DataTable` + `Paginator`**, with columns as data. Detail
   pages are `DetailHeader` + stacked `Section`s about one subject.
5. **Every visible string is a `t()` key** in `src/locales/en/*.json`.

---

## 0. Backend (only if the endpoints don't exist yet)

Add admin endpoints to the owning Rust service (`odo-auth` / `odo-org` /
`odo-notify` / `odo-asset`), following its existing admin modules:

- Use the `odo_service::admin` helpers: `clean_required` / `clean_code` /
  `clean_optional` / `clean_email` / `clean_search` for input,
  `contains_ci` for search (SeaORM's `Column::contains` is a
  case-sensitive `LIKE`), `map_unique_violation` for 409s, and
  `Page` / `Paginated<T>` / `Sort` for lists.
- Declare a paginated response with the macro; a bare
  `type XPage = Paginated<Row>` alias does NOT get a row-typed schema:

  ```rust
  odo_service::page_type!(WidgetPage, WidgetRow, "One page of widgets.");
  // handler: Ok(Json(Paginated::new(rows, total).into()))
  ```

- Resolve `Sort` against an explicit allow-list, never mapping the client
  string to a column, and add a stable tiebreaker:

  ```rust
  let (col, ord) = params.sort.resolve(
      &[("code", Column::Code), ("description", Column::Description)],
      (Column::Code, Order::Asc),   // default + fallback for unknown keys
  );
  query.order_by(col, ord).order_by_asc(Column::Id)
  ```

- In update requests, a null field means "unchanged". If a nullable field
  must be clearable, add a `clear_<field>: bool` and refuse both at once
  with `check_clear`.
- Gate each handler on its `read` / `write` permission, and register its
  schemas and paths in the service's `bin/<service>.rs` `ApiDoc`.
- **Derived, read-only views** (anything computed from authz and the org
  tree, like effective permission scope) belong in a SQL function that
  reuses the enforcement path's expressions, so display can't drift from
  enforcement: see `authz.usr_perm_scopes` (in `001_odo_baseline`), which
  mirrors `authz.usr_has_perm_at`, and its pgTAP tests in
  `tests/db/odo/usr_perm_scopes_tests.sql` (`./scripts/run-tests.sh --db`).
- **Add integration tests** for every new endpoint in
  `tests/integration/tests/`: authn (401), authz (403), validation, and
  the behaviour itself. Tests run concurrently against shared data, so
  create what you assert on and don't assume a list fits on one page.

Then regenerate the committed specs and the UI's types:

```bash
scripts/generate-openapi.sh          # specs + src/ui/odo-admin/src/api/types/*.ts
scripts/generate-openapi.sh --check  # what CI runs; must be clean
```

## 1. Permission

Add the tool's codes to **`src/auth/perms.ts`**, mirroring the sqitch
change that seeds them and grants them to the `odo-admin` role:

```ts
WIDGET_READ: "odo.widget.read",
WIDGET_WRITE: "odo.widget.write",
```

## 2. API client

Add a typed client in **`src/api/<service>.ts`** (or extend the existing
one), typed from the generated schemas:

```ts
import { apiPost } from "./client"
import type { components } from "./types/odo-widget"

type S = components["schemas"]
export type WidgetRow = S["WidgetRow"]

export const widgetsApi = {
  listPage(params: Partial<S["ListWidgetsRequest"]>): Promise<S["WidgetPage"]> {
    return apiPost("/api/v1/odo/widget/admin/widget/list", params)
  },
  /** Every widget, for pickers: lists default to 50 rows. */
  listAll(): Promise<WidgetRow[]> {
    return fetchAllPages((page) => widgetsApi.listPage(page))
  },
  create(params: S["CreateWidgetRequest"]): Promise<WidgetRow> {
    return apiPost("/api/v1/odo/widget/admin/widget/create", params)
  },
}
```

Errors arrive as `ApiRequestError` (`status`, `code`, `field`); show them
with `describeError(err)`. Add a widely shared error code to
`src/api/errors.ts` and `errors.codes.*` in `common.json`.

## 3. Strings

Create **`src/locales/en/<tool>.json`** for the tool's namespace, register
it in **`src/i18n/i18next.d.ts`** (keys are type-checked), and add the
tool's `label` and `description` under `tools.<id>` in `common.json`.

## 4. Routes

Create **`src/routes/<tool>/`** with one module per route. Each exports a
`loader` and a `Component`, and optionally a `handle` naming its
breadcrumb:

```tsx
const LIST: ListDefaults = {
  sort: { key: "code", dir: "asc" },
  sortKeys: ["code", "label"],          // the backend's allow-list
}

export async function loader({ request }: LoaderFunctionArgs) {
  const url = new URL(request.url)
  const state = readListState(url.searchParams, LIST)
  const [page, perms] = await Promise.all([
    widgetsApi.listPage(listRequest(state)),
    checkPerms({ canWrite: PERMS.WIDGET_WRITE }),
    loadNamespaces("widgets"),
  ])
  redirectPastLastPage(url, state, page.total, LIST)
  return { page, ...perms }
}

export function Component() {
  const { page, canWrite } = useLoaderData<typeof loader>()
  const { t } = useTranslation("widgets")
  const [list, setList] = useListState(LIST)
  const columns: Column<WidgetRow>[] = [
    {
      id: "code",
      header: t("columns.code"),
      sortKey: "code",
      cell: (w) => <Link to={String(w.id)}>{w.code}</Link>,
    },
    // a computed column (a count) has no sortKey
  ]
  return (
    <div className="flex flex-col gap-4">
      <PageHeader title={t("title")} description={t("subtitle")} />
      <SearchInput value={list.q} onCommit={(q) => setList({ q })} label={t("search")} />
      <DataTable rows={page.rows} columns={columns} rowKey={(w) => w.id}
        rowHref={(w) => String(w.id)} sort={list.sort}
        onSortChange={(sort) => setList({ sort })} empty={t("empty")} busy={useBusy()} />
      <Paginator total={page.total} pageIndex={list.pageIndex}
        pageSize={list.pageSize} onChange={setList} />
    </div>
  )
}
```

Building blocks to reuse rather than rewrite:

| Need | Use |
|---|---|
| List table, sort headers, paging | `DataTable`, `Paginator`, `lib/list-state` |
| Debounced search box bound to the URL | `SearchInput` |
| Detail page header (back link, title, facts, actions) | `DetailHeader` |
| A facet of a detail page | `Section` |
| A bounded sub-list with create/edit/delete dialogs | `FacetSection` |
| "Are you sure?" | `ConfirmDialog` |
| A disabled action that says why | `DisabledReason` |
| A searchable picker for long lists | `Combobox` |
| Status label (icon/tone + text) | `StatusBadge` |
| Headline counts | `StatTile` |
| Dates, times, counts in the user's locale | `useFormatters` |
| Reporting an unexpected error | `useReportError` |

Dialogs remount their form on each open (render the form only while open)
so state never leaks between uses; see `routes/roles/create-role-dialog.tsx`.
After a write, call `useRevalidator().revalidate()` rather than patching
loader data by hand.

## 5. Register the tool

Add one entry to `ADMIN_TOOLS` in **`src/lib/admin-tools.ts`** and the id
to `ToolId`. That wires the routes (behind the read-perm middleware), the
sidebar entry, the breadcrumb, and the home card:

```ts
{
  id: "widgets",
  path: "widgets",
  icon: PuzzleIcon,                      // a lucide-react icon
  readPerm: PERMS.WIDGET_READ,
  children: [
    { index: true, lazy: () => import("@/routes/widgets") },
    { path: ":widgetId", lazy: () => import("@/routes/widgets/detail") },
  ],
},
```

## 6. Verify

```bash
# from src/ui/odo-admin (Node >= 22.22; build ../core first)
npm run typecheck
npm run lint
npm test
npm run build
npm run dev    # http://localhost:5173, API proxied to the dev cluster
```

Add unit tests for any pure logic (parsing, mapping, rules) beside it as
`*.test.ts`. Try the `ar-XB` pseudo-locale from the account menu: it
shows layout that isn't direction-aware.

## 7. E2E

Add the tool to the smoke list in
`tests/e2e/apps/odo-admin/tests/smoke.spec.ts` (its nav label and the
heading its page renders), using roles and accessible names rather than
CSS classes. Run it against the cluster, or against `npm run dev`:

```bash
# from tests/e2e
BASE_URL=http://localhost:30080 npm run test:odo-admin
```

## 8. Deploy

Backend changes: `./scripts/build-and-deploy-service.sh <service>`, then
wait ~20s. UI changes:

```bash
./scripts/build-and-deploy-service.sh ui-odo-admin --wait
```

No new Envoy route is needed for a tool: the SPA is one app under
`/odo/admin`, and new endpoints under an existing `/api/v1/odo/<service>`
prefix are already routed by `odo-routes.yaml`. Only a brand-new path
prefix needs a route there (and an entry in the README's routing registry).
