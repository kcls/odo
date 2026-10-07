-- Deploy odo-demo:002_demo_email_groups to pg
-- requires: 001_demo_org_tree

-- Sample notification email groups, one per demo region plus a
-- system-wide security group, so apps' alert routing has somewhere to
-- send on a demo install.
--
-- Groups carry pinned uuids (5eed0000-...-03xx) because apps reference
-- them across the database boundary -- Current's demo routing rules name
-- these exact uuids.
--
-- Every member address is under example.org, which RFC 2606 reserves
-- and which publishes a null MX: mail to it is never delivered, so a
-- demo or e2e run that triggers an alert emails nobody.

BEGIN;

INSERT INTO notification.email_group (code, label, uuid) VALUES
    ('demo-east-region', 'East Region Staff', '5eed0000-0000-4000-a000-000000000301'),
    ('demo-west-region', 'West Region Staff', '5eed0000-0000-4000-a000-000000000302'),
    ('demo-security',    'Security Team',     '5eed0000-0000-4000-a000-000000000303')
ON CONFLICT (uuid) DO UPDATE SET
    code = EXCLUDED.code, label = EXCLUDED.label;

INSERT INTO notification.email_group_member (email_group, email)
SELECT g.id, m.email
  FROM (VALUES
    ('5eed0000-0000-4000-a000-000000000301'::uuid, 'east.manager@example.org'),
    ('5eed0000-0000-4000-a000-000000000301'::uuid, 'east.supervisor@example.org'),
    ('5eed0000-0000-4000-a000-000000000302'::uuid, 'west.manager@example.org'),
    ('5eed0000-0000-4000-a000-000000000302'::uuid, 'west.supervisor@example.org'),
    ('5eed0000-0000-4000-a000-000000000303'::uuid, 'security@example.org')
  ) AS m (group_uuid, email)
  JOIN notification.email_group g ON g.uuid = m.group_uuid
ON CONFLICT (email_group, email) DO NOTHING;

COMMIT;
