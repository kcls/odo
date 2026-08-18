-- Deploy odo:002_odo_seed to pg
-- requires: 001_odo_baseline

-- Platform seed for a fresh odo install: the odo.* permissions, the
-- platform roles (odo-admin, odo-notify-service, odo-registration) and their grants, a small
-- generic org tree exercising every unit type, and the machine
-- accounts (dev-default passwords -- change in production).
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
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Root', NULL, true, false, '5eed0000-0000-4000-a000-000000000101');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Region', (SELECT id FROM org.unit_type WHERE label = 'Root'), true, false,
     '5eed0000-0000-4000-a000-000000000102');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Branch', (SELECT id FROM org.unit_type WHERE label = 'Region'), true, true,
     '5eed0000-0000-4000-a000-000000000103');
INSERT INTO org.unit_type (label, parent, can_have_staff, can_have_patrons, uuid) VALUES
    ('Locker', (SELECT id FROM org.unit_type WHERE label = 'Branch'), false, false,
     '5eed0000-0000-4000-a000-000000000104');

-- ---- org.unit --------------------------------------------------------------

-- A small generic sample tree that exercises every unit type:
--
--   Odo Library System (Root, OLS)
--   ├── East Region (ERG)
--   │   ├── Main Street Branch (MAIN)
--   │   │   └── Main Street Locker (MAINL)
--   │   └── Riverside Branch (RIVR)
--   └── West Region (WRG)
--       ├── Hilltop Branch (HILL)
--       └── Lakeside Branch (LAKE)
--
-- Parents resolved by code; no hard-coded ids.
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Odo Library System', 'OLS', NULL,
     (SELECT id FROM org.unit_type WHERE label = 'Root'),
     NULL, '5eed0000-0000-4000-a000-000000000201');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('East Region', 'ERG', (SELECT id FROM org.unit WHERE code = 'OLS'),
     (SELECT id FROM org.unit_type WHERE label = 'Region'),
     NULL, '5eed0000-0000-4000-a000-000000000202');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('West Region', 'WRG', (SELECT id FROM org.unit WHERE code = 'OLS'),
     (SELECT id FROM org.unit_type WHERE label = 'Region'),
     NULL, '5eed0000-0000-4000-a000-000000000203');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Main Street Branch', 'MAIN', (SELECT id FROM org.unit WHERE code = 'ERG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000204');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Riverside Branch', 'RIVR', (SELECT id FROM org.unit WHERE code = 'ERG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000205');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Hilltop Branch', 'HILL', (SELECT id FROM org.unit WHERE code = 'WRG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000206');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Lakeside Branch', 'LAKE', (SELECT id FROM org.unit WHERE code = 'WRG'),
     (SELECT id FROM org.unit_type WHERE label = 'Branch'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000207');
INSERT INTO org.unit (label, code, parent, unit_type, timezone, uuid) VALUES
    ('Main Street Locker', 'MAINL', (SELECT id FROM org.unit WHERE code = 'MAIN'),
     (SELECT id FROM org.unit_type WHERE label = 'Locker'),
     'America/Los_Angeles', '5eed0000-0000-4000-a000-000000000208');

-- ---- accounts --------------------------------------------------------------

-- odo-notify-service: shared low-privilege machine account used by
-- application background jobs to authenticate and enqueue notifications.
-- Dev-default password; change in production.
INSERT INTO auth.usr (username, email, auth_method, status, display_name, uuid)
VALUES ('odo-notify-service', 'odo-notify-service@odo.example.org', 'local', 'active', '',
        '5eed0000-0000-4000-a000-000000000001');

-- TODO would it make more sense to apply this password only
-- via the test data scripts?
INSERT INTO auth.local_account (usr, password_hash)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-notify-service'),
        crypt('odo-notify-service-dev-only', gen_salt('bf', 10)));

INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-notify-service'),
        'odo-notify-service',
        (SELECT id FROM org.root()));

-- odo-registration: machine account apps use to register their seed data
-- via the odo APIs. Dev-default password; change in production.
INSERT INTO auth.usr (username, email, auth_method, status, display_name, uuid)
VALUES ('odo-registration', 'odo-registration@odo.example.org', 'local', 'active', '',
        '5eed0000-0000-4000-a000-000000000002');

INSERT INTO auth.local_account (usr, password_hash)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-registration'),
        crypt('odo-registration-dev-only', gen_salt('bf', 10)));

INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
VALUES ((SELECT id FROM auth.usr WHERE username = 'odo-registration'),
        'odo-registration',
        (SELECT id FROM org.root()));

COMMIT;
