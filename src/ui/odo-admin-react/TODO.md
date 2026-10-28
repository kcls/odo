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

## Stages to come

- [ ] Org Units: tree list, detail with addresses, closures, hours
- [ ] Email Templates: list, editor with preview
- [ ] Email Groups: list, detail with members
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
- [ ] `@odo/core`'s `userHasPerm` returns false on any error, and the
      result is cached for the session, so a transient failure hides a
      tool until the next sign-in. Same as the Angular app.

## Cutover

- [ ] Port the remaining tools above
- [ ] Point e2e (`tests/e2e/apps/odo-admin`) at the new UI and update
      selectors; the User Roles smoke entry goes away
- [ ] Move `src/ui/odo-admin-react` to `src/ui/odo-admin`, delete the
      Angular app, update `service-map.yaml` (dockerfile path),
      `scripts/generate-openapi.sh` (TS_OUT_DIRS), CI, and CLAUDE.md
- [ ] The k8s deployment's probes hit `/odo/admin`; with the new nginx
      config that returns a 301 to `/odo/admin/`. Probes accept 3xx, but
      pointing them at `/odo/admin/` is cleaner
- [ ] Update `docs/adding-a-tool.md` for the React app
