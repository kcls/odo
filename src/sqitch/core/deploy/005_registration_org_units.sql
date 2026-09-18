-- Deploy odo:005_registration_org_units to pg
-- requires: 002_odo_seed

-- Let the odo-registration account manage org structure.
--
-- 002_odo_seed deliberately withheld this: org structure was "the
-- platform's", and an app's registration manifest had no business
-- re-parenting a library system's branches.
--
-- What changed is that an installation's own site data -- its real org
-- tree -- has to be installed somehow, and the alternative was raw SQL
-- against the odo database for the one thing every install must define.
-- Manifests are upsert-only, permission-checked and address units by
-- natural key, which is strictly better than hand-written INSERTs.
--
-- The trade is real and worth stating plainly: the account every app
-- registration runs under can now create org units and unit types. It
-- still cannot delete or re-parent anything through a manifest --
-- odo-register only ever POSTs to */create -- but the write permission
-- itself is broader than that. If app-supplied manifests ever become
-- less trusted than they are today, this is the grant to split back out
-- into a separate site-data account.
--
-- Idempotent: ON CONFLICT DO NOTHING, so re-running is a no-op.

BEGIN;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-registration', 'odo.org.unit.read', 0),
    ('odo-registration', 'odo.org.unit.write', 0)
ON CONFLICT DO NOTHING;

COMMIT;
