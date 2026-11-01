# odo-admin: TODO

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
      The mapping role picker lists every role.

## New features

- [x] **Create user accounts** from the Users page ("New user", gated
      on `odo.auth.user.write`): password sign-in or pre-provisioned
      SAML, then straight to the new user's page to assign roles. Local
      accounts get a password (8+ characters, confirmed) at creation;
      SAML accounts never send one. No pinned uuids from the UI (that's
      for fixtures and registration).
- [x] **Set or reset a local user's password**: "Set password" on the
      user's page, backed by odo-auth `user/set-password`
      (`odo.auth.user.write`). It gives a passwordless account one, clears
      any lockout, and ends the user's sessions (sparing the caller's
      own when resetting your own). Refused for SAML accounts and
      deleted ones.
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
- [x] **Notifications: per-day chart and retry.** A stacked bar chart
      of deliveries per day by outcome (counted in the viewer's time
      zone, quiet days included), with a table view kept in the URL,
      from odo-notify `delivery/daily`. A Retry action on a failed or
      skipped email delivery's page puts it back in the queue with a
      fresh retry budget, from `delivery/retry` behind the new
      `odo.notify.delivery.write` (sqitch `009_notify_delivery_write`).
- [x] **Deactivate or reactivate an account** from the user's page,
      SAML accounts included (their profile still belongs to the IdP, but
      access is odo's): `user/update` takes `status` (`active` or
      `inactive`), refuses your own account, and ends the user's sessions
      on deactivation. The Users list searches active or inactive accounts
      (`?status=inactive`), since `user/search` matches one status.
- [x] **odo-manager role** (sqitch `011_odo_manager_role`): every
      platform read permission, user detail included, plus
      `odo.notify.email_group.write` and `odo.notify.template.write`.
      Not delivery retry, and no other writes. The UI hides each write
      action without its permission, so the role sees every tool read-only
      apart from Email Groups and Email Templates.
- [x] **Email template history**: every write to a template is kept as a
      version (sqitch `012_notify_template_history`; a trigger, so app
      registration and direct SQL are captured too, with the author when
      the writer names one). A template's History page lists its
      versions; a version's page shows what it changed and how it differs
      from the template now, and restores it (itself a new version). Backed
      by odo-notify `template/{versions,version,restore}`. Live templates
      only: a deleted template's versions are kept but not shown.

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
- [x] Template lookup by id paged through the whole list (odo-notify
      had no `template/get`, and the Angular editor's `limit: 1000` was
      clamped to 200). Fixed: `template/get`, which the editor uses.
- [x] `UserAccountRow` (user/detail) lacked `second_given_name`, so the
      edit dialog couldn't show the middle name, and the Angular dialog
      cleared it on every save by sending `""`. Fixed: user/detail,
      update and create return it, and the dialog edits it.
- [x] odo-auth `UserResponse` (user/search, user/get) Debug-formatted
      the status, so it arrived as `"\"active\""`. Fixed: the stored value
      is returned as is. Nothing depended on the quoted form; kcls/current
      compares `status === 'active'`, which had been false for everyone.
- [x] Picker lists were silently capped at 50: admin list endpoints
      default to 50 rows (max 200), and the Angular app's "all roles" and
      "all permissions" pickers never passed a limit. The React pickers
      page through everything (`api/paging.ts`) and are searchable.
- [x] Fields that couldn't be cleared once set (null meant "unchanged"):
      a unit type's parent, an IdP's session lifetime and attribute
      mapping, an SP's IdP. Fixed with explicit flags (`clear_parent`,
      `clear_session_lifetime_hours`, `clear_attribute_mapping`,
      `clear_idp`; a value and its clear flag together are refused), so
      existing callers sending null are unaffected. The admin dialogs
      offer "None"/blank again.
- [ ] The Users page's org-unit picker uses `@odo/core`'s per-session
      tree cache, so units added or renamed in Org Units don't appear
      there until a reload. Org Units itself always reads fresh.
- [x] **Account status wasn't enforced.** Password login, SAML login, and
      token refresh never checked `auth.usr.status`; only the unused (and
      broken: it reads a nonexistent `is_deleted`) SQL function
      `auth.verify_user_credentials` did, so the `odo-registration`
      account's documented `inactive` lockdown did nothing. odo-auth now
      requires `active` on all three (SAML refusals redirect with
      `?error=account_inactive`), and sqitch
      `010_drop_verify_user_credentials` drops the function.
- [x] **A soft-deleted account could keep signing in**: SAML login found
      the user by email without checking `deleted_at`, and token refresh
      didn't check it either (nor did deleting end sessions). Fixed: SAML
      refuses an email whose only accounts are deleted
      (`?error=account_inactive`) rather than recreating it, refresh
      requires a live account, and deleting ends the user's sessions.
- [ ] `@odo/core`'s `userHasPerm` returns false on any error, and the
      result is cached for the session, so a transient failure hides a
      tool until the next sign-in. Same as the Angular app.

## Cutover

- [x] Port every Angular tool
- [x] Move the app to `src/ui/odo-admin` and delete the Angular app;
      repoint `scripts/generate-openapi.sh`, CI, CLAUDE.md, and the k8s
      probes (now `/odo/admin/`, which no longer redirects)
- [x] Rewrite `docs/adding-a-tool.md` for this app
- [x] **Overhaul the e2e tests** (`tests/e2e/apps/odo-admin`) against
      roles and accessible names: smoke for every tool, permission
      gating, server sorting, deep links and back/forward, the post-login
      redirect, effective permissions, assigning and removing a role, the
      template preview and unsaved-changes guard, org unit child
      navigation. 23 pass against the dev server at `/odo/admin/`.
- [x] Deploy (`./scripts/build-and-deploy-service.sh ui-odo-admin --wait`)
      and run the e2e suite against the cluster
      (`cd tests/e2e && BASE_URL=http://localhost:30080 npm run test:odo-admin`)
- [ ] Serving at a base path other than `/odo/admin` is checked by hand
      (the container accepts `ODO_ADMIN_BASE_PATH`); e2e runs against
      one deployment, so it can't cover it
