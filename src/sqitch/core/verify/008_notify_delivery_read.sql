-- Verify odo:008_notify_delivery_read on pg

BEGIN;

-- The permission exists and odo-admin holds it. The CASE keeps the
-- aggregate row alive so the division runs (see verify/005).
SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM authz.permission
 WHERE code = 'odo.notify.delivery.read';

SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-admin'
   AND perm = 'odo.notify.delivery.read';

SELECT 1/(CASE WHEN COUNT(*) = 1 THEN 1 ELSE 0 END)
  FROM pg_indexes
 WHERE schemaname = 'notification'
   AND indexname = 'idx_delivery_created_at';

ROLLBACK;
