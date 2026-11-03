-- Revert odo:013_registration_read_perms from pg

BEGIN;

DELETE FROM authz.role_permission
 WHERE role = 'odo-registration'
   AND perm IN ('odo.auth.role.read', 'odo.auth.user.read',
                'odo.auth.user_role.read', 'odo.notify.template.read',
                'odo.asset.directory.read');

COMMIT;
