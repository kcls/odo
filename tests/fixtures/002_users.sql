-- E2E test users. No hard-coded database ids: rows are referenced by
-- username and carry pinned UUIDs (prefix e2e00000-...) for stable
-- cross-install references:
--
--   e2e.odo.staff     e2e00000-0000-4000-a000-000000000001
--   e2e.odo.sso       e2e00000-0000-4000-a000-000000000002
--   e2e.odo.admin  e2e00000-0000-4000-a000-000000000003
--   e2e.odo.mutable   e2e00000-0000-4000-a000-000000000005
--
-- Local login users (password: test123!):
--   e2e.odo.staff    - e2e-test-role @ root (can log in; no other perms)
--   e2e.odo.admin - odo-admin @ root
--
-- SSO users (MockSAML - any password works):
--   e2e.odo.sso@example.com - e2e-test-role @ root (can log in; no other perms)
--
-- e2e.odo.mutable is the guinea pig for user-admin mutation tests (rename,
-- soft-delete/restore): a local account nothing logs in as, so mutating
-- it can't race other tests. Re-running this file resets it.
--
-- App test users (e.g. Current's incident-role users) are NOT defined
-- here: apps own their fixtures.
--
-- Idempotent: safe to re-run (resets passwords and role assignments).
-- Requires 001_test_role.sql (e2e-test-role).

BEGIN;

-- Un-delete first (keyed on uuid): a crashed mutation test can leave
-- e2e.odo.mutable soft-deleted, which would escape the partial unique index
-- below and produce a duplicate active row on the next run.
UPDATE auth.usr SET deleted_at = NULL
 WHERE uuid = 'e2e00000-0000-4000-a000-000000000005' AND deleted_at IS NOT NULL;

-- Upsert by username (partial unique index over active users). Users are
-- never hard-deleted (the platform forbids it), so re-runs update in place.
INSERT INTO auth.usr (username, email, auth_method, status, first_given_name, family_name, uuid)
VALUES
    ('e2e.odo.staff', 'e2e.odo.staff@odo.example.org', 'local', 'active', 'E2E', 'Staff',
     'e2e00000-0000-4000-a000-000000000001'),
    ('e2e.odo.sso', 'e2e.odo.sso@example.com', 'saml', 'active', 'E2E', 'SSO',
     'e2e00000-0000-4000-a000-000000000002'),
    ('e2e.odo.admin', 'e2e.odo.admin@odo.example.org', 'local', 'active', 'E2E', 'OdoAdmin',
     'e2e00000-0000-4000-a000-000000000003'),
    ('e2e.odo.mutable', 'e2e.odo.mutable@odo.example.org', 'local', 'active', 'E2E', 'Mutable',
     'e2e00000-0000-4000-a000-000000000005')
ON CONFLICT (username) WHERE deleted_at IS NULL DO UPDATE SET
    email = EXCLUDED.email,
    auth_method = EXCLUDED.auth_method,
    status = EXCLUDED.status,
    first_given_name = EXCLUDED.first_given_name,
    family_name = EXCLUDED.family_name,
    uuid = EXCLUDED.uuid;

-- Reset passwords to test123! for the local-login users.
DELETE FROM auth.local_account
 WHERE usr IN (SELECT id FROM auth.usr WHERE username IN ('e2e.odo.staff', 'e2e.odo.admin'));

INSERT INTO auth.local_account (usr, password_hash)
SELECT id, crypt('test123!', gen_salt('bf'))
  FROM auth.usr
 WHERE username IN ('e2e.odo.staff', 'e2e.odo.admin');

-- Reset role assignments to exactly the fixture set.
DELETE FROM authz.usr_role_org_map
 WHERE usr IN (SELECT id FROM auth.usr
                WHERE username IN ('e2e.odo.staff', 'e2e.odo.sso', 'e2e.odo.admin'));

INSERT INTO authz.usr_role_org_map (usr, role, org_unit)
VALUES
    ((SELECT id FROM auth.usr WHERE username = 'e2e.odo.staff'),
     'e2e-test-role', (SELECT id FROM org.root())),
    ((SELECT id FROM auth.usr WHERE username = 'e2e.odo.sso'),
     'e2e-test-role', (SELECT id FROM org.root())),
    ((SELECT id FROM auth.usr WHERE username = 'e2e.odo.admin'),
     'odo-admin', (SELECT id FROM org.root()));

COMMIT;
