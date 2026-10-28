-- Deploy odo:008_notify_delivery_read to pg
-- requires: 002_odo_seed

-- Let admins see notification delivery history: who was sent what, when,
-- and what failed (odo-notify's delivery admin endpoints, the admin UI's
-- Notifications page).
--
-- Read-only on purpose. Deliveries hold rendered message bodies, so the
-- permission is separate from template and email-group access rather
-- than implied by either; it goes to odo-admin, like every platform
-- permission.
--
-- The index serves those endpoints' time windows ("the last 7 days"),
-- which otherwise scan the whole table; the existing indexes are all
-- keyed for the processor's queue or for one recipient.
--
-- Idempotent: ON CONFLICT DO NOTHING / IF NOT EXISTS.

BEGIN;

INSERT INTO authz.permission (code, description) VALUES
    ('odo.notify.delivery.read', 'View notification delivery history, including rendered messages and failures')
ON CONFLICT DO NOTHING;

INSERT INTO authz.role_permission (role, perm, min_depth) VALUES
    ('odo-admin', 'odo.notify.delivery.read', 0)
ON CONFLICT DO NOTHING;

CREATE INDEX IF NOT EXISTS idx_delivery_created_at
    ON notification.delivery USING btree (created_at);

COMMIT;
