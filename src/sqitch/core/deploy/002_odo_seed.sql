-- Deploy odo:002_odo_seed to pg
-- requires: 001_odo_baseline

-- Root org unit identity. Defaults reproduce the demo install; an
-- installation overrides them (see the org.unit section below).
\if :{?root_code}
\else
  \set root_code OLS
\endif
\if :{?root_label}
\else
  \set root_label 'Odo Library System'
\endif
\if :{?root_uuid}
\else
  \set root_uuid 5eed0000-0000-4000-a000-000000000201
\endif

-- Platform seed for a fresh odo install: the odo.* permissions, the
-- platform roles (odo-admin, odo-notify-service, odo-registration) and
-- their grants, the org unit types, the root org unit (parameterized --
-- see below), and the machine accounts (both ship with unknowable
-- passwords; dev and CI get known ones from src/test-data/).
--
-- The demo org tree below the root is NOT here: it lives in the separate
-- `demo` sqitch project (src/sqitch/demo), so an installation deploying
-- into a real org structure can skip it.
--
-- No hard-coded database ids: rows are created with generated ids and
-- referenced by natural keys (code/label/username) or by their pinned
-- UUIDs. Seeded rows carry well-known UUIDs (prefix 5eed0000-...) so
-- fixtures and tests can reference them stably across installs:
--
--   unit types  5eed0000-0000-4000-a000-0000000001xx
--   org units   5eed0000-0000-4000-a000-0000000002xx
--   accounts    5eed0000-0000-4000-a000-0000000000xx
--
-- Application-originated data (an app's permissions, roles, templates,
-- SAML attribute->role mappings, asset directories) is NOT seeded here:
-- apps register it themselves -- today via their own sqitch change against
-- this database (e.g. kcls/current sqitch/odo-data), eventually via a
-- declarative registration API. e2e fixtures live in src/test-data.

BEGIN;

-- ---- authz.permission ------------------------------------------------------

INSERT INTO authz.permission (code, description) VALUES
    ('odo.notify.send', 'Send notifications via the notification service'),
    ('odo.auth.session', 'Create authentication sessions (AKA login)'),
    ('odo.auth.user.read', 'View basic staff/user information'),
    ('odo.notify.email_group.read', 'View notification email groups and their members'),
    ('odo.notify.email_group.write', 'Manage notification email groups and their members'),
    ('odo.notify.template.read', 'View notification templates'),
    ('odo.notify.template.write', 'Manage notification templates'),
    ('odo.auth.role.read', 'View roles, permissions, and role permission grants'),
    ('odo.auth.role.write', 'Manage roles, permissions, and role permission grants'),
    ('odo.auth.user_role.read', 'View user role assignments'),
    ('odo.auth.user_role.write', 'Assign and remove user roles; checked at the assignment org unit'),
    ('odo.auth.saml.read', 'View SAML identity and service provider configuration'),
    ('odo.auth.saml.write', 'Manage SAML identity and service provider configuration'),
    ('odo.auth.user.detail.read', 'View detailed user account info: sessions, SAML identities, and role assignments'),
    ('odo.auth.user.write', 'Edit local user accounts (names, deletion)'),
    ('odo.org.unit.read', 'View org units, unit types, and their addresses/closures/hours'),
    ('odo.org.unit.write', 'Manage org units, unit types, and their addresses/closures/hours'),
    ('odo.asset.directory.read', 'View the asset directory registry'),
    ('odo.asset.directory.write', 'Register and remove asset directories (apps register their own alongside the permissions those directories reference)');

-- ---- authz.role ------------------------------------------------------------

INSERT INTO authz.role (code, description, label) VALUES
    ('odo-notify-service', 'Machine account for background jobs (notification enqueue)', 'Notify Service Account'),
    ('odo-registration', 'Machine account role holding exactly the permissions app registration needs', 'App Registration Service Account'),
    ('odo-admin', 'Administrative access to ODO admin UIs', 'ODO Administrator');

-- ---- authz.role_permission -------------------------------------------------

-- odo-notify-service: just enough to log in and enqueue notifications.
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-notify-service', 'odo.notify.send', 0),
    ('odo-notify-service', 'odo.auth.session', 0);

-- odo-registration: exactly the permissions app registration needs.
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-registration', 'odo.auth.session', 0),
    ('odo-registration', 'odo.auth.role.write', 0),
    ('odo-registration', 'odo.auth.user.write', 0),
    ('odo-registration', 'odo.auth.user_role.write', 0),
    ('odo-registration', 'odo.auth.saml.read', 0),
    ('odo-registration', 'odo.auth.saml.write', 0),
    ('odo-registration', 'odo.notify.template.write', 0),
    ('odo-registration', 'odo.notify.email_group.write', 0),
    ('odo-registration', 'odo.asset.directory.write', 0);

-- odo-admin: every platform permission. (Asset-directory permissions are
-- app-registered alongside the directories that reference them, e.g.
-- odo.asset.current.* arrives with kcls/current's registration.)
INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-admin', 'odo.notify.send', 0),
    ('odo-admin', 'odo.auth.session', 0),
    ('odo-admin', 'odo.auth.user.read', 0),
    ('odo-admin', 'odo.notify.email_group.read', 0),
    ('odo-admin', 'odo.notify.email_group.write', 0),
    ('odo-admin', 'odo.notify.template.read', 0),
    ('odo-admin', 'odo.notify.template.write', 0),
    ('odo-admin', 'odo.auth.role.read', 0),
    ('odo-admin', 'odo.auth.role.write', 0),
    ('odo-admin', 'odo.auth.user_role.read', 0),
    ('odo-admin', 'odo.auth.user_role.write', 0),
    ('odo-admin', 'odo.auth.saml.read', 0),
    ('odo-admin', 'odo.auth.saml.write', 0),
    ('odo-admin', 'odo.auth.user.detail.read', 0),
    ('odo-admin', 'odo.auth.user.write', 0),
    ('odo-admin', 'odo.org.unit.read', 0),
    ('odo-admin', 'odo.org.unit.write', 0),
    ('odo-admin', 'odo.asset.directory.read', 0),
    ('odo-admin', 'odo.asset.directory.write', 0);

-- ---- org.unit_type ---------------------------------------------------------

-- Root -> Region -> Branch -> Locker. Parents resolved by label; no
-- hard-coded ids.
--
-- Root and Region are organizational groupings, not places anyone works,
-- so neither can hold staff: an application asking "can staff be here?"
-- (Current's Communication Log does, before accepting an entry) must get
-- the same answer from a demo install as from a real one. A Branch is
-- the physical location; a Locker is unstaffed self-service.
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Root', NULL, false, false, '5eed0000-0000-4000-a000-000000000101');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Region', (SELECT id FROM org.unit_type WHERE label = 'Root'), false, false,
     '5eed0000-0000-4000-a000-000000000102');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Branch', (SELECT id FROM org.unit_type WHERE label = 'Region'), true, true,
     '5eed0000-0000-4000-a000-000000000103');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Locker', (SELECT id FROM org.unit_type WHERE label = 'Branch'), false, false,
     '5eed0000-0000-4000-a000-000000000104');

-- ---- org.unit (root only) --------------------------------------------------

-- The root org unit, and only the root. Every other unit belongs to an
-- installation, and an installation registers its own tree through
-- odo-register manifests -- except that it cannot register a root:
-- unit/create takes a non-optional parent, and org.unit carries a
-- single-root unique index. So the root has to be seeded, and a real
-- installation's root is not "Odo Library System".
--
-- Hence three deploy-time variables, defaulting to the demo values:
--
--   sqitch deploy -s root_code=KCLS -s root_label='KCLS' \
--                 -s root_uuid=5c2a0a5b-...
--
-- or, better for an installation that deploys repeatedly, in its
-- sqitch.conf so `sqitch verify` sees them too:
--
--   [core "variables"]
--       root_code = KCLS
--
-- Deploying without them produces the demo root, which is what a public
-- install, the e2e suites and a new developer's checkout all want.
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    (:'root_label', :'root_code', NULL,
     (SELECT id FROM org.unit_type WHERE label = 'Root'),
     NULL, :'root_uuid');

-- ---- accounts --------------------------------------------------------------

-- odo-notify-service: shared low-privilege machine account used by
-- application background jobs to authenticate and enqueue notifications.
--
-- Ships with an unknowable password, like odo-registration: not a
-- placeholder anyone could guess, and not the same value in every
-- install. An operator sets a real one (manage-secrets.sh
-- update-notify-service writes the Kubernetes secret and prints the
-- UPDATE that sets the matching hash); dev and CI get a known password
-- from src/test-data/ instead.
--
-- Unlike odo-registration this account stays 'active'. It is not a
-- bootstrap account activated for the length of one run -- application
-- background jobs log into it continuously, so disabling it would just
-- break the draft reminder until someone re-enabled it. The password is
-- the control here, not the status flag.
--
-- A random hash rather than NULL on purpose: a null password_hash would
-- leave verify_password() comparing against SQL NULL, and a real hash
-- for an unknown secret is the safer failure mode.
INSERT INTO auth.usr (username, email, auth_method, status, display_name, uuid)
VALUES ('odo-notify-service', 'odo-notify-service@odo.example.org', 'local', 'active', '',
        '5eed0000-0000-4000-a000-000000000001');

INSERT INTO auth.local_account (usr, password_hash)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-notify-service'),
        crypt(gen_random_uuid()::text || gen_random_uuid()::text, gen_salt('bf', 10)));

INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-notify-service'),
        'odo-notify-service',
        (SELECT id FROM org.root()));

-- odo-registration: machine account used to register app seed data via
-- the odo APIs. It holds write permissions across auth, notify and asset
-- (see the grants above), so it ships switched off:
--
--   * status 'inactive' -- auth.verify_user_credentials() filters on
--     status = 'active', so local login is refused in the database, not
--     merely in the application layer.
--   * an unknowable password -- not a placeholder anyone could guess and
--     not the same value in every install.
--
-- scripts/load-data-manifest.sh activates the account, sets a password it
-- generates, applies the manifests and switches it back off. Dev and CI
-- get a known password from src/test-data/ instead; a production install
-- that never loads test data has no usable credential for this account.
--
-- A random hash rather than NULL on purpose: a null password_hash would
-- leave verify_password() comparing against SQL NULL, and a real hash for
-- an unknown secret is the safer failure mode.
INSERT INTO auth.usr (username, email, auth_method, status, display_name, uuid)
VALUES ('odo-registration', 'odo-registration@odo.example.org', 'local', 'inactive', '',
        '5eed0000-0000-4000-a000-000000000002');

INSERT INTO auth.local_account (usr, password_hash)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-registration'),
        crypt(gen_random_uuid()::text || gen_random_uuid()::text, gen_salt('bf', 10)));

INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-registration'),
        'odo-registration',
        (SELECT id FROM org.root()));

COMMIT;
