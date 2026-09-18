-- Revert odo-demo:001_demo_org_tree from pg
--
-- Deepest first, so no row is removed while another still references it.
-- Deletes by the pinned demo uuids rather than by code: a code could
-- collide with an installation's own unit, a uuid in this range cannot.
--
-- org.unit carries a hard-delete guard (001_odo_baseline), hence the
-- explicit, transaction-local opt-out -- this is a revert of the change
-- that created these rows, so there is nothing to preserve.

BEGIN;

SET LOCAL app.allow_hard_delete = 'on';

DELETE FROM org.unit WHERE uuid IN (
    '5eed0000-0000-4000-a000-000000000208',  -- Main Street Locker
    '5eed0000-0000-4000-a000-000000000204',  -- Main Street Branch
    '5eed0000-0000-4000-a000-000000000205',  -- Riverside Branch
    '5eed0000-0000-4000-a000-000000000206',  -- Hilltop Branch
    '5eed0000-0000-4000-a000-000000000207',  -- Lakeside Branch
    '5eed0000-0000-4000-a000-000000000202',  -- East Region
    '5eed0000-0000-4000-a000-000000000203'   -- West Region
);

COMMIT;
