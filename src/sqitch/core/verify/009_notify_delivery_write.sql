-- Verify odo:009_notify_delivery_write on pg

BEGIN;

-- The permission exists and odo-admin holds it. The CASE keeps the
-- aggregate row alive so the division runs (see verify/005).
SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM authz.permission
 WHERE code = 'odo.notify.delivery.write';

SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-admin'
   AND perm = 'odo.notify.delivery.write';

ROLLBACK;
