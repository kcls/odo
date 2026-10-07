-- Verify odo-demo:002_demo_email_groups on pg

BEGIN;

DO $$
DECLARE
    n INTEGER;
BEGIN
    SELECT COUNT(*) INTO n FROM notification.email_group
     WHERE uuid::text LIKE '5eed0000-0000-4000-a000-0000000003%'
       AND deleted_at IS NULL;
    IF n <> 3 THEN
        RAISE EXCEPTION 'expected 3 demo email groups, found %', n;
    END IF;

    -- The point of the domain: no demo address may be deliverable.
    IF EXISTS (
        SELECT 1 FROM notification.email_group_member m
          JOIN notification.email_group g ON g.id = m.email_group
         WHERE g.uuid::text LIKE '5eed0000-0000-4000-a000-0000000003%'
           AND m.email NOT LIKE '%@example.org'
    ) THEN
        RAISE EXCEPTION 'demo email group member outside example.org';
    END IF;
END $$;

ROLLBACK;
