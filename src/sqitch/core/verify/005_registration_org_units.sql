-- Verify odo:005_registration_org_units on pg

BEGIN;

-- Both grants are present. The CASE keeps the aggregate row alive so the
-- division actually runs: a HAVING clause here would filter the row away
-- when the count is wrong, and the verify would pass with nothing granted.
SELECT 1/(CASE WHEN COUNT(*) = 2 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-registration'
   AND perm IN ('odo.org.unit.read', 'odo.org.unit.write');

ROLLBACK;
