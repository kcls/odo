-- Verify odo:005_registration_org_units on pg

BEGIN;

-- Both grants are present.
SELECT 1/COUNT(*) FROM authz.role_permission
 WHERE role = 'odo-registration'
   AND perm IN ('odo.org.unit.read', 'odo.org.unit.write')
HAVING COUNT(*) = 2;

ROLLBACK;
