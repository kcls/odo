-- Revert odo:014_odo_viewer_notify_admin from pg
--
-- Restores odo-manager as 011 left it, but does not move anyone back:
-- the split roles' holders can't be mapped onto it without widening
-- someone's access (an odo-notify-admin-only holder would gain every
-- read). So, like 011's revert, this fails while any user holds
-- odo-viewer or odo-notify-admin (usr_role_org_map has no cascade):
-- reassign those users deliberately first. SAML attribute mappings to
-- the two roles go with them (that foreign key cascades).

BEGIN;

INSERT INTO authz.role (code, description, label) VALUES
    ('odo-manager', 'Read access to the ODO admin UI; manages email groups and email templates', 'ODO Manager')
ON CONFLICT DO NOTHING;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-manager', 'odo.auth.session', 0),
    ('odo-manager', 'odo.auth.user.read', 0),
    ('odo-manager', 'odo.auth.user.detail.read', 0),
    ('odo-manager', 'odo.auth.role.read', 0),
    ('odo-manager', 'odo.auth.user_role.read', 0),
    ('odo-manager', 'odo.auth.saml.read', 0),
    ('odo-manager', 'odo.org.unit.read', 0),
    ('odo-manager', 'odo.asset.directory.read', 0),
    ('odo-manager', 'odo.notify.delivery.read', 0),
    ('odo-manager', 'odo.notify.email_group.read', 0),
    ('odo-manager', 'odo.notify.email_group.write', 0),
    ('odo-manager', 'odo.notify.template.read', 0),
    ('odo-manager', 'odo.notify.template.write', 0)
ON CONFLICT DO NOTHING;

DELETE FROM authz.role_permission WHERE role IN ('odo-viewer', 'odo-notify-admin');
DELETE FROM authz.role WHERE code IN ('odo-viewer', 'odo-notify-admin');

COMMIT;
