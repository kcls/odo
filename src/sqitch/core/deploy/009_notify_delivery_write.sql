-- Deploy odo:009_notify_delivery_write to pg
-- requires: 008_notify_delivery_read

-- Let admins retry a failed or skipped notification delivery from the
-- admin UI's Notifications page (odo-notify delivery/retry): it puts the
-- delivery back in the send queue. Granted to odo-admin, like every
-- platform permission; separate from delivery.read so seeing history
-- doesn't imply resending mail.
--
-- Idempotent: ON CONFLICT DO NOTHING.

BEGIN;

INSERT INTO authz.permission (code, description) VALUES
    ('odo.notify.delivery.write', 'Retry failed or skipped notification deliveries')
ON CONFLICT DO NOTHING;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-admin', 'odo.notify.delivery.write', 0)
ON CONFLICT DO NOTHING;

COMMIT;
