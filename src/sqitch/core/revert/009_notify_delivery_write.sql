-- Revert odo:009_notify_delivery_write from pg

BEGIN;

-- Grants first: role_permission references the permission.
DELETE FROM authz.role_permission WHERE perm = 'odo.notify.delivery.write';
DELETE FROM authz.permission WHERE code = 'odo.notify.delivery.write';

COMMIT;
