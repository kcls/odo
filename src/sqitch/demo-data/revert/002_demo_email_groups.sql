-- Revert odo-demo:002_demo_email_groups from pg
--
-- Deletes by the pinned demo uuids; members go with their group (ON
-- DELETE CASCADE). email_group carries a hard-delete guard, hence the
-- transaction-local opt-out -- this reverts the change that created the
-- rows. A group that deliveries now reference fails on the foreign key,
-- correctly: its history is no longer disposable.

BEGIN;

SET LOCAL app.allow_hard_delete = 'on';

DELETE FROM notification.email_group WHERE uuid IN (
    '5eed0000-0000-4000-a000-000000000301',  -- East Region Staff
    '5eed0000-0000-4000-a000-000000000302',  -- West Region Staff
    '5eed0000-0000-4000-a000-000000000303'   -- Security Team
);

COMMIT;
