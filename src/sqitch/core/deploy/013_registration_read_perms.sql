-- Deploy odo:013_registration_read_perms to pg
-- requires: 005_registration_org_units

-- Let the odo-registration account read what it registers.
--
-- odo-register used to POST every manifest item and treat a 409 as
-- "already registered", so the account only ever needed write
-- permissions. That told the operator nothing before a run, and nothing
-- afterwards about rows that exist with values other than the
-- manifest's (which registration never changes). `odo-register --plan`
-- reads the current state first and reports what a run would create,
-- what is already there, and what differs, so it needs read access to
-- the same things it writes.
--
-- Each read is narrower than the write the account already holds, with
-- one exception worth stating: odo.auth.user.read lets it search all
-- users, where before it could only create them. The account stays
-- disabled outside load-data-manifest.sh runs.
--
-- Idempotent: ON CONFLICT DO NOTHING, so re-running is a no-op.

BEGIN;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-registration', 'odo.auth.role.read', 0),
    ('odo-registration', 'odo.auth.user.read', 0),
    ('odo-registration', 'odo.auth.user_role.read', 0),
    ('odo-registration', 'odo.notify.template.read', 0),
    ('odo-registration', 'odo.asset.directory.read', 0)
ON CONFLICT DO NOTHING;

COMMIT;
