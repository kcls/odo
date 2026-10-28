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
- [x] SAML: identity providers, their service providers, attributes,
      and role mappings (paginated, state in the URL). Changes from
      Angular: SPs linked to no IdP were invisible there; they're now
      listed on the SAML page and can be linked from their edit dialog.
      Fields the API can't clear once set (an IdP's session lifetime and
      attribute mapping, an SP's IdP) are required once set, with the
      reason shown, instead of offering a blank that silently does
      nothing. The mapping role picker lists every role.

## New features

- [x] **Create user accounts** from the Users page ("New user", gated
      on `odo.auth.user.write`): password sign-in or pre-provisioned
      SAML, then straight to the new user's page to assign roles. Local
      accounts require a password (8+ characters, confirmed), because
      nothing can set one later; SAML accounts never send one. No pinned
      uuids from the UI (that's for fixtures and registration).
- [ ] **Set or reset a local user's password.** odo-auth has no
      endpoint for it, so a forgotten password, or an account created
      without one (only possible via the API), needs SQL. Needs an
      admin endpoint behind `odo.auth.user.write` (hashing with
      `auth.hash_password`, clearing `locked_until` and failed attempts),
      then a "Set password" action on the user's page.
- [x] **Notifications page**: outcome tiles (delivered, failed, skipped,
      queued) for a time range and channel, each linking to those
      deliveries; per-recipient activity (totals, failures, last sent,
      last failure; sortable); the delivery list (outcome, subject, and
      recipient filters, partial group failures called out); and a page
      per delivery with its outcome, per-address results, the message as
      sent, and the event behind it. Backed by new read-only odo-notify
      endpoints `delivery/{list,get,summary,recipients}` behind
      `odo.notify.delivery.read` (sqitch `008_notify_delivery_read`, which
      also indexes `delivery.created_at`), with integration tests in
      `tests/integration/tests/delivery_admin.rs`.
- [ ] Possible next steps for Notifications: a daily volume chart, and a
      retry action for failed deliveries (needs a write permission and
      endpoint).

## Known issues found while porting

- [x] **Admin searches were case-sensitive.** `clean_search`'s doc said
      SeaORM's `Column::contains` was a case-insensitive ILIKE; it's a
      plain `LIKE`. odo-service adds `admin::contains_ci` (escaped ILIKE)
      and `escape_like`, now used by the permission, role, template,
      email group, and unit type searches; SAML mapping and delivery
      search share the escaping instead of their own copies.
- [x] **odo-auth `create_user` inserted the account before validating
      the password**, so a short one left a passwordless account behind
      and a retry got `USERNAME_TAKEN`. Fixed: the password is checked
      before any write, and the account and its password are written in
      one transaction.
- [x] **Emails escaped HTML in subjects and plain-text bodies.** Fixed:
      odo-service adds `template::render_text` (no escaping) and
      `text_to_html`; odo-notify renders subjects and in-app bodies as
      text, HTML bodies with escaped values, and text-only email as the
      verbatim text escaped whole with line breaks kept. The preview
      endpoint matches. `render` is unchanged for existing callers.
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
