-- Deploy odo:011_odo_manager_role to pg
-- requires: 009_notify_delivery_write

-- odo-manager: sees everything the admin UI shows, but changes only
-- email groups and email templates. For staff who maintain notification
-- content and recipients without being trusted with accounts, roles,
-- SAML, or the org tree.
--
-- Every platform read permission (user detail included, which exposes
-- session IPs) plus the two notification writes. Deliberately not:
-- odo.notify.send, odo.notify.delivery.write (retrying deliveries), and
-- the other *.write permissions. Like odo-admin it should be assigned at
-- the root org unit; grants are min_depth 0.
--
-- Idempotent: ON CONFLICT DO NOTHING.

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

COMMIT;
