-- Verify odo:013_registration_read_perms on pg

BEGIN;

-- All five grants are present. CASE rather than HAVING, as in 005: a
-- HAVING clause would drop the row and pass with nothing granted.
SELECT 1/(CASE WHEN COUNT(*) = 5 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-registration'
   AND perm IN ('odo.auth.role.read', 'odo.auth.user.read',
                'odo.auth.user_role.read', 'odo.notify.template.read',
                'odo.asset.directory.read');

ROLLBACK;
