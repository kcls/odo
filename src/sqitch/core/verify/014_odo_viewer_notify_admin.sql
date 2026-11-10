-- Verify odo:014_odo_viewer_notify_admin on pg

BEGIN;

-- Both roles exist, and odo-notify-admin holds the two writes it's for.
-- No exact grant lists: admins may change the roles' grants in the UI
-- afterwards. The CASE keeps the aggregate row alive so the division
-- runs (see verify/005).
SELECT 1/(CASE WHEN COUNT(*) = 2 THEN 1 ELSE 0 END)
  FROM authz.role
 WHERE code IN ('odo-viewer', 'odo-notify-admin');

SELECT 1/(CASE WHEN COUNT(*) = 2 THEN 1 ELSE 0 END)
  FROM authz.role_permission
 WHERE role = 'odo-notify-admin'
   AND perm IN ('odo.notify.email_group.write', 'odo.notify.template.write');

ROLLBACK;
