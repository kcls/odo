-- Revert odo:011_odo_manager_role from pg
--
-- Fails while any user holds the role (usr_role_org_map has no cascade):
-- remove those assignments deliberately first rather than have a revert
-- silently strip access. SAML attribute mappings to the role go with it
-- (that foreign key cascades).

BEGIN;

DELETE FROM authz.role_permission WHERE role = 'odo-manager';
DELETE FROM authz.role WHERE code = 'odo-manager';

COMMIT;
