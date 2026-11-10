-- Deploy odo:014_odo_viewer_notify_admin to pg
-- requires: 011_odo_manager_role

-- Split odo-manager (011) in two, so read access to the admin UI and
-- control of notifications can be granted separately:
--
--   odo-viewer        every platform read permission (user detail
--                     included, which exposes session IPs); no writes.
--   odo-notify-admin  email templates and email groups, read and write,
--                     and the Notifications page (delivery history). Not
--                     delivery retry. Complete on its own: it carries
--                     odo.auth.session, so it can sign in without
--                     odo-viewer.
--
-- Every odo-manager assignment and SAML attribute mapping becomes one of
-- each, at the same org unit with the same SAML flag, so no holder loses
-- access; then odo-manager is removed. Like odo-admin, both should be
-- assigned at the root org unit; grants are min_depth 0.
--
-- Idempotent: ON CONFLICT DO NOTHING, and the odo-manager deletes match
-- nothing on a second run.

BEGIN;

INSERT INTO authz.role (code, description, label) VALUES
    ('odo-viewer', 'Read-only access to the ODO admin UI', 'ODO Viewer'),
    ('odo-notify-admin', 'Manages notification templates and email groups, and views notification delivery history', 'ODO Notification Admin')
ON CONFLICT DO NOTHING;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-viewer', 'odo.auth.session', 0),
    ('odo-viewer', 'odo.auth.user.read', 0),
    ('odo-viewer', 'odo.auth.user.detail.read', 0),
    ('odo-viewer', 'odo.auth.role.read', 0),
    ('odo-viewer', 'odo.auth.user_role.read', 0),
    ('odo-viewer', 'odo.auth.saml.read', 0),
    ('odo-viewer', 'odo.org.unit.read', 0),
    ('odo-viewer', 'odo.asset.directory.read', 0),
    ('odo-viewer', 'odo.notify.delivery.read', 0),
    ('odo-viewer', 'odo.notify.email_group.read', 0),
    ('odo-viewer', 'odo.notify.template.read', 0),
    ('odo-notify-admin', 'odo.auth.session', 0),
    ('odo-notify-admin', 'odo.notify.delivery.read', 0),
    ('odo-notify-admin', 'odo.notify.email_group.read', 0),
    ('odo-notify-admin', 'odo.notify.email_group.write', 0),
    ('odo-notify-admin', 'odo.notify.template.read', 0),
    ('odo-notify-admin', 'odo.notify.template.write', 0)
ON CONFLICT DO NOTHING;

INSERT INTO authz.usr_role_org_map (usr, role, org_unit, is_managed_by_saml)
SELECT m.usr, r.code, m.org_unit, m.is_managed_by_saml
  FROM authz.usr_role_org_map m
 CROSS JOIN (VALUES ('odo-viewer'), ('odo-notify-admin')) AS r(code)
 WHERE m.role = 'odo-manager'
ON CONFLICT DO NOTHING;

INSERT INTO authz.saml_attr_role_map (is_active, attr, role, attr_value)
SELECT m.is_active, m.attr, r.code, m.attr_value
  FROM authz.saml_attr_role_map m
 CROSS JOIN (VALUES ('odo-viewer'), ('odo-notify-admin')) AS r(code)
 WHERE m.role = 'odo-manager'
ON CONFLICT DO NOTHING;

DELETE FROM authz.usr_role_org_map WHERE role = 'odo-manager';
DELETE FROM authz.saml_attr_role_map WHERE role = 'odo-manager';
DELETE FROM authz.role_permission WHERE role = 'odo-manager';
DELETE FROM authz.role WHERE code = 'odo-manager';

COMMIT;
