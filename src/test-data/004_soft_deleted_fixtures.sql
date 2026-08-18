-- Soft-deleted fixtures so integration tests can exercise the
-- resolve-by-id "include deleted" paths (odo-org label-batch / GET unit,
-- odo-auth get_user). There is no HTTP path to soft-delete an org unit or
-- user, so we seed them pre-deleted (inserting with deleted_at set is not
-- a DELETE, so the hard-delete guard doesn't fire).
--
-- Pinned UUIDs (tests resolve rows by uuid, never by database id):
--
--   deleted branch  e2e00000-0000-4000-a000-000000000101
--   e2e.odo.deleted     e2e00000-0000-4000-a000-000000000004
--
-- Idempotent: keyed on uuid (deleted rows escape the partial unique
-- indexes on username/code, so uuid is the stable identity here).

BEGIN;

-- Soft-deleted branch under the root: excluded from the active
-- tree/scoping, resolvable only via include_deleted.
INSERT INTO org.unit (label, code, parent, unit_type, timezone, deleted_at, uuid)
SELECT 'E2E Deleted Branch', 'E2EDEL',
       (SELECT id FROM org.root()),
       (SELECT id FROM org.unit_type WHERE label = 'Branch'),
       'America/Los_Angeles', now(),
       'e2e00000-0000-4000-a000-000000000101'
 WHERE NOT EXISTS (SELECT 1 FROM org.unit
                    WHERE uuid = 'e2e00000-0000-4000-a000-000000000101');

-- Soft-deleted user (no local_account; not meant to log in).
INSERT INTO auth.usr (username, email, auth_method, status,
                      first_given_name, family_name, deleted_at, uuid)
SELECT 'e2e.odo.deleted', 'e2e.odo.deleted@odo.example.org', 'local', 'active',
       'E2E', 'Deleted', now(),
       'e2e00000-0000-4000-a000-000000000004'
 WHERE NOT EXISTS (SELECT 1 FROM auth.usr
                    WHERE uuid = 'e2e00000-0000-4000-a000-000000000004');

COMMIT;
