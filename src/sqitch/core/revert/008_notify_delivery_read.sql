-- Revert odo:008_notify_delivery_read from pg

BEGIN;

DROP INDEX IF EXISTS notification.idx_delivery_created_at;

-- Grants first: role_permission references the permission.
DELETE FROM authz.role_permission WHERE perm = 'odo.notify.delivery.read';
DELETE FROM authz.permission WHERE code = 'odo.notify.delivery.read';

COMMIT;
