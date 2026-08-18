# Adding a new admin tool

This app is a **reference implementation**: every tool is built the same way,
from the same reusable parts, so adding one is mostly configuration plus a few
small files — never hand-rolled plumbing. Read one existing tool end to end
first; **`features/org-units/`** is the canonical example (list + drill-down
detail + dialogs + a feature-local typed API client), and the simpler
**`features/permissions/`** shows a single-list tool.

The golden rules that make this app maintainable:

1. **Generated types are the source of truth.** Never hand-write an interface
   that mirrors a backend struct. Import from `core/api-types/<service>`.
2. **Tables are `<odo-table>`** (CDK-based), never Material's `mat-table`;
   paginate them with `<odo-paginator>`, never `mat-paginator`.
3. **Detail pages are `<odo-detail-page>`** with stacked `<section>`s — the user
   should feel they are drilling into one subject, not reading disconnected lists.
4. **Style with Material's token API only.** No `.mat-mdc-*` / `.mdc-*` selectors
   in stylesheets; reuse the global utility classes.
5. **Server-driven lists.** Growing lists are paginated (`{ rows, total }`) and
   sorted server-side (allow-listed columns only); gate every write affordance
   on the tool's `*_WRITE` perm.

---

## 0. Backend (only if the endpoints don't exist yet)

If your tool needs new admin endpoints, add them to the owning Rust service
(`odo-auth` / `odo-org` / `odo-notify` / `odo-asset`) following the existing
admin modules:

- Use the `odo::admin` helpers: `clean_required`/`clean_code`/`clean_optional`/
  `clean_email`/`clean_search` for input, `map_unique_violation` for 409s,
  `Page`/`Paginated<T>` for list inputs/outputs.
- For a **paginated** list, declare its response type with the macro — a bare
  `type XPage = Paginated<Row>` alias will NOT get a row-typed OpenAPI schema:

  ```rust
  odo::page_type!(WidgetPage, WidgetRow, "One page of widgets.");
  // handler: Ok(Json(Paginated::new(rows, total).into()))
  ```

- Gate each handler with a `read`/`write` permission check.
- Register new schemas + paths in the service's `bin/<service>.rs` `ApiDoc`.
- **Derived/read-only views** (anything computed from authz + the org tree, e.g.
  effective permission scope): put the logic in a SQL function in a sqitch
  migration and have the handler just call it, rather than re-deriving in Rust or
  TypeScript. Reuse the *same* expressions the enforcement path uses so the
  display can't drift from what's actually enforced — see
  `authz.usr_perm_scopes` (migration 092), which mirrors `authz.usr_has_perm_at`,
  and its pgTAP coverage in `src/db-tests/usr_perm_scopes_tests.sql`. Add pgTAP
  tests for any non-trivial SQL function; run them with
  `PGPASSWORD=… PGHOST=localhost PGPORT=5432 ./run-tests.sh` from `src/db-tests`.

Then regenerate the committed specs + TypeScript types:

```bash
scripts/generate-openapi.sh          # dumps specs, regenerates api-types/*.ts
scripts/generate-openapi.sh --check  # what CI runs; must be clean
```

## 1. Permission

Add the tool's `read`/`write` codes to **`core/perms.ts`** (mirroring the codes
seeded by sqitch change `090_odo_admin_role`), and make sure that migration
seeds them and grants them to the `odo-admin` role.

```ts
WIDGET_READ: 'widget.read',
WIDGET_WRITE: 'widget.write',
```

## 2. Feature folder

Create `features/<tool>/` with:

- **`<tool>-api.ts`** — the typed client. Copy the shape from
  `features/org-units/org-admin-api.ts`:

  ```ts
  import { apiGet, apiPost } from '../../core/api';
  import type { components } from '../../core/api-types/odo-<service>';
  type S = components['schemas'];

  export type WidgetRow = S['WidgetRow'];
  export type WidgetPage = S['WidgetPage'];

  export const widgetApi = {
    async list(search?: string): Promise<WidgetRow[]> {
      const res = await apiPost<WidgetPage>('/api/v1/odo/<svc>/admin/widget/list', {
        search: search || undefined,
      });
      return res.rows;            // paginated list -> read .rows
    },
    create(params: S['CreateWidgetRequest']): Promise<WidgetRow> {
      return apiPost('/api/v1/odo/<svc>/admin/widget/create', params);
    },
    // update / delete ...
  };
  ```

- **`<tool>.routes.ts`** — export a `Routes` const: `''` → list component,
  and `':id'` → detail component if the tool drills in.

- **List component** (`<tool>-list.ts` + `.html` + `.scss`):
  - Import `CdkTableModule` **and** `OdoTable` (the column defs live in the
    host template, so the host must bring `CdkTableModule`).
  - Load into signals in `ngOnInit`; use `ErrorHandlerService.show(err, '…')`.
  - Set a `canWrite` signal from `auth.hasPerm(PERMS.WIDGET_WRITE)`.
  - Render `<odo-table [rows] [displayedColumns] [clickableRows] (rowClick)>`
    with projected `cdkColumnDef` columns. `.scss` is usually just
    `:host { display: block; }`.

  - For a **server-paginated** list, place an `<odo-paginator>` directly beneath
    the `<odo-table>` and drive both from server state:

    ```html
    <odo-table [rows]="rows()" [displayedColumns]="cols" />
    <odo-paginator
      [length]="total()"
      [pageIndex]="pageIndex()"
      [pageSize]="pageSize()"
      (page)="onPage($event)"
    />
    ```

    ```ts
    onPage(e: OdoPageEvent) {           // { pageIndex, pageSize }
      this.pageIndex.set(e.pageIndex);
      this.pageSize.set(e.pageSize);
      void this.reload();               // re-fetch with limit/offset
    }
    ```

    Use `limit: pageSize`, `offset: pageIndex * pageSize` in the request, and
    set `total()` from the response's `total`. See `features/saml/idp-detail`.

  - For **server-driven sorting**, keep a `sort` signal, bind it to
    `<odo-table>`, wrap sortable headers in `<odo-sort-header key="...">`, and
    send `sort_by`/`sort_dir` in the request. Only allow-listed **real DB
    columns** are sortable — computed/aggregate columns (counts) render a plain
    header. See `features/permissions/permission-list` (the reference).

    ```html
    <odo-table [rows]="rows()" [displayedColumns]="cols"
               [sort]="sort()" (sortChange)="onSort($event)">
      <ng-container cdkColumnDef="code">
        <th cdk-header-cell *cdkHeaderCellDef>
          <odo-sort-header key="code" i18n>Code</odo-sort-header>
        </th>
        <td cdk-cell *cdkCellDef="let r">{{ r.code }}</td>
      </ng-container>
      <!-- a computed column: plain header, not sortable -->
      <ng-container cdkColumnDef="roles">
        <th cdk-header-cell *cdkHeaderCellDef i18n>Roles</th>
        <td cdk-cell *cdkCellDef="let r">{{ r.role_count }}</td>
      </ng-container>
    </odo-table>
    ```

    ```ts
    onSort(s: OdoSort) {                 // { active, direction }
      this.sort.set(s);
      this.pageIndex.set(0);            // sort change resets to page 1
      void this.reload();
    }
    ```

    On the **backend**, flatten `odo::admin::Sort` into the request struct and
    resolve it against an explicit allow-list — never map the client string to a
    column directly (injection/enumeration risk). Append a stable tiebreaker
    (usually the PK) unless the sort column is already unique:

    ```rust
    let (col, ord) = params.sort.resolve(
        &[("code", Column::Code), ("description", Column::Description)],
        (Column::Code, Order::Asc),   // default + fallback for unknown keys
    );
    query.order_by(col, ord).order_by_asc(Column::Id)
    ```

- **Detail component** (if drilling in) — wrap in `<odo-detail-page>` with
  `backLink`/`backLabel`/`title`, a `[slot=summary]`, an optional
  `[slot=actions]` (write-gated), and a body of stacked
  `<section class="section">` blocks, each a `.section-header` (`<h2>` +
  optional `.page-intro` + action button) followed by an `<odo-table>`.

- **Dialogs** — copy a dialog from `features/org-units/` (e.g. `unit-dialog.ts`).
  Use documented Material form modules, `ServerErrorStateMatcher`, and map known
  `ApiRequestError.code`s to field errors.

Reuse the global classes from `styles.scss`: `.page-header`, `.page-intro`,
`.section`, `.section-header`, `.list-loading`, `.list-empty`, `.cell-code`,
`.cell-actions`, `.status-badge` + `.status-*`.

## 3. Register the tool

Append one entry to `ADMIN_TOOLS` in **`core/admin-tools.ts`**. This wires the
lazy route, the sidenav entry, **and** the home-page card at once:

```ts
{
  path: 'widgets',
  icon: 'widgets',                     // a Material Symbols name
  label: $localize`:Admin tool name:Widgets`,
  description: $localize`:Admin tool description:Manage widgets.`,
  readPerm: PERMS.WIDGET_READ,
  loadChildren: () =>
    import('../features/widgets/widgets.routes').then((m) => m.WIDGET_ROUTES),
},
```

That's the only wiring step — routing (`app.routes.ts`), nav, and home grid all
read the registry and are permission-filtered automatically.

## 4. Verify

```bash
# from src/ui/odo-admin (Node 24 via nvm)
npm run build        # must be clean: no errors AND no NG8116 warnings
npm test             # component specs
```

Check your feature dir has **no** `mat-table`/`MatTableModule` and **no**
`.mat-mdc-*`/`.mdc-*` selectors.

## 5. E2E

Add the tool to the smoke list in
`src/e2e/apps/odo-admin/tests/smoke.spec.ts` (its nav label + a heading its
list page renders). Run against a deployed cluster:

```bash
# from src/e2e
BASE_URL=http://localhost:30080 npm run test:odo-admin
```

## 6. Deploy

Backend changes: `./scripts/build-and-deploy-service.sh <service>` and wait
~20s. UI changes:

```bash
./scripts/build-service.sh ui-odo-admin
./scripts/deploy-service.sh ui-odo-admin --wait
```

(The combo `build-and-deploy-service.sh ui-odo-admin` also works but doesn't
accept `--wait` — it forwards the flag to the build step, which errors after
the image already pushed.)

No new envoy route is needed for a tool — the SPA is one app under
`/odo/admin`, and its `/api/v1/odo/*` calls are already routed. New *backend*
endpoints under an existing `/api/v1/odo/<service>` prefix are already covered
by `odo-routes.yaml`; only a brand-new path prefix would need a route there.
