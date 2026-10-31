-- Verify odo:011_odo_manager_role on pg

BEGIN;

-- The role exists and holds the two writes it's for. No exact grant list:
-- admins may change the role's grants in the UI afterwards. The CASE
-- keeps the aggregate row alive so the division runs (see verify/005).
SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM authz.role
 WHERE code = 'odo-manager';

SELECT 1/(CASE WHEN COUNT(*) = 2 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-manager'
   AND perm IN ('odo.notify.email_group.write', 'odo.notify.template.write');

ROLLBACK;
