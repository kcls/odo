-- Revert odo:002_odo_seed from pg

-- The seed is data-only; reverting truncates every seeded table. On a
-- dev database this is equivalent to a reset back to bare schema.

BEGIN;

TRUNCATE TABLE authz.usr_role_org_map, auth.local_account, auth.usr,
    authz.role_permission, authz.role, authz.permission,
    org.unit, org.unit_type
    RESTART IDENTITY CASCADE;

COMMIT;
