-- Revert odo:005_registration_org_units from pg

BEGIN;

DELETE FROM authz.role_permission
 WHERE role = 'odo-registration'
   AND perm IN ('odo.org.unit.read', 'odo.org.unit.write');

COMMIT;
