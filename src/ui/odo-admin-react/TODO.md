# odo-admin-react: TODO

## Ported

- [x] Shell: sidebar from the tool registry, breadcrumbs, theme and
      language menus, sign-out, session-expiry redirect
- [x] Login: local, SAML SSO buttons, SAML callback (`?sso=1`) and
      error codes, `redirect_to` (same-app paths only)
- [x] Home: cards for the tools the user may open
- [x] Shared list table, paginator, URL list state, detail header,
      confirm dialog
- [x] Permissions (reference server-driven list)
- [x] Users: search, detail, edit name, mark deleted/restore
- [x] User roles merged into the Users detail page (assign/remove,
      effective permissions). There is no separate User Roles tool.
- [x] Slate theme (from odo-translate 984e197, with the contrast fixes
      from f409ffd it was tuned against)
- [x] Roles: list, create, detail, edit, delete (blocked while
      assigned), permission grants with inline min-depth editing
- [x] Org Unit Types: list, create, edit, delete (blocked while in use)
- [x] Org Units: indented tree with a filter (in the URL, client-side),
      detail with child units, addresses, closures, and hours. Changes
      from Angular: delete moved from the list to the unit's page
      (blocked for the root and units with children, with the reason
      shown); "Add child unit" presets the parent; timezone is picked
      from the browser's IANA list; days and times format per locale;
      closures sort newest first and mark past ones.
- [x] Email Templates: list, and an editor with the form beside a live
      preview (debounced as you type) and a guard against leaving with
      unsaved edits. The HTML preview renders in an empty-sandbox
      iframe: the Angular app relied on Angular's innerHTML
      sanitizing, React has none, and templates can contain script.
- [x] Email Groups: list (inactive groups behind a filter kept in the
      URL), create, edit, deactivate/reactivate, and members managed in
      place (add inline, switch on or off, remove; a filter appears for
      large groups)

## Stages to come

- [ ] SAML: IdP list/detail, attributes, role mappings, SP dialogs

## New features

- [ ] **Create user accounts** from the Users page. `POST
      /api/v1/odo/auth/user/create` already exists (`CreateUserRequest`:
      username, email, names, auth_method, optional password, optional
      pinned uuid). Needs: a "New user" dialog gated on
      `odo.auth.user.write`, local vs. pre-provisioned SAML choice,
      optional initial password (or none, so the account can't sign in
      until one is set), then go to the new user's detail page to
      assign roles. Decide whether the UI should ever pin a uuid
      (probably not; that's for fixtures and registration).
- [ ] **Email notifications page**: which groups and users get emails,
      when, and what failed. `notification.delivery` already records
      channel, status, scheduled/processed times, retry_count and
      max_retries, error_code and error_message, and the recipient (a
      user or an email group), linked to `notification.event`
      (template, source service/entity). There is no admin read API
      yet. Needs, in odo-notify: a paginated, sortable, filterable
      delivery list (status, channel, template, recipient, date range)
      via `page_type!`; a delivery detail (rendered title/body, error,
      retries, event); a summary (counts by status over a window,
      recent failures); a read perm (e.g.
      `odo.notify.delivery.read`) seeded and granted to `odo-admin`;
      routing is covered by the existing `/api/v1/odo/notify` prefix.
      Possibly a retry action later, behind a write perm. Then the
      page: summary, failures first, filters in the URL, links to the
      email group or user.

## Known issues found while porting

- [ ] **Emails escape HTML in subjects and plain-text bodies.**
      `odo_service::template::render` uses Handlebars' default HTML
      escaping, and odo-notify's enqueue path renders subjects and
      plain bodies with it, so a variable like `O'Brien` or `Smith &
      Sons` arrives as `O&#x27;Brien` / `Smith &amp; Sons`. Only the
      HTML body should escape. Fix in odo-service: a non-escaping
      renderer (`hbs.register_escape_fn(handlebars::no_escape)`) for
      subject and text bodies, with tests; the preview endpoint shares
      it, so the admin preview will follow.
- [ ] Template lookup by id pages through the whole list: odo-notify
      has no `template/get`. The Angular editor asked for
      `limit: 1000`, which the server clamps to 200, so templates past
      the 200th couldn't be opened there. Fine at today's counts; a get
      endpoint would be better.

- [ ] `UserAccountRow` (user/detail) lacks `second_given_name`, so the
      edit dialog can't show the middle name. The Angular dialog
      always sent `second_given_name: ""`, which clears it on every
      save. The React dialog leaves middle name out until the backend
      returns it.
- [ ] odo-auth `UserResponse` (user/search, user/get) Debug-formats
      the status (`format!("{s:?}")` on a String), so it arrives as
      `"\"active\""`. The Users list shows it verbatim, as Angular did.
      The fix is one line in `src/rust/odo-auth/src/user.rs`, but check
      consumers (kcls/current) for reliance on the quoted form first.
- [x] Picker lists were silently capped at 50: admin list endpoints
      default to 50 rows (max 200), and the Angular app's "all roles" and
      "all permissions" pickers never passed a limit. The React pickers
      page through everything (`api/paging.ts`) and are searchable.
- [ ] odo-org can't clear a unit type's parent: `UpdateUnitTypeRequest`
      treats a null `parent` as "unchanged". The Angular dialog offered
      "None" anyway and silently did nothing. The React dialog doesn't
      offer "None" once a type has a parent and says why. Backend fix:
      accept an explicit clear (e.g. a `clear_parent` flag, or
      distinguish an absent field from null).
- [ ] The Users page's org-unit picker uses `@odo/core`'s per-session
      tree cache, so units added or renamed in Org Units don't appear
      there until a reload. Org Units itself always reads fresh.
- [ ] `@odo/core`'s `userHasPerm` returns false on any error, and the
      result is cached for the session, so a transient failure hides a
      tool until the next sign-in. Same as the Angular app.

## Cutover

- [ ] Port the remaining tools above
- [ ] **Overhaul the e2e tests** (`tests/e2e/apps/odo-admin`) once the
      UI is done. Every spec is tied to Angular internals (`tr.odo-row`,
      `odo-table`, `odo-sort-header`, `mat-icon` text, `.shell-nav-link`,
      `.app-error-snackbar`, `td.cell-code`), so rewrite them against
      roles and accessible names (`getByRole`, `aria-sort`) rather than
      repointing selectors. Carry over smoke, negative-perm, sorting,
      effective permissions (now on the Users page; the User Roles entry
      goes away) and org-unit child navigation. Add coverage for what's
      new: list state in the URL (deep links, back/forward), assigning
      and removing roles from the Users page, the template editor's
      unsaved-changes guard and preview, the session-expiry redirect
      back to the original page, and serving at a base path other than
      `/odo/admin`.
- [ ] Move `src/ui/odo-admin-react` to `src/ui/odo-admin`, delete the
      Angular app, update `service-map.yaml` (dockerfile path),
      `scripts/generate-openapi.sh` (TS_OUT_DIRS), CI, and CLAUDE.md
- [ ] The k8s deployment's probes hit `/odo/admin`; with the new nginx
      config that returns a 301 to `/odo/admin/`. Probes accept 3xx, but
      pointing them at `/odo/admin/` is cleaner
- [ ] Update `docs/adding-a-tool.md` for the React app
