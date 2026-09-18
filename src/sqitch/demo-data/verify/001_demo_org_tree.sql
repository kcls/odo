-- Verify odo-demo:001_demo_org_tree on pg

BEGIN;

DO $$
DECLARE
    n INTEGER;
BEGIN
    SELECT COUNT(*) INTO n FROM org.unit
     WHERE uuid::text LIKE '5eed0000-0000-4000-a000-0000000002%'
       AND parent IS NOT NULL
       AND deleted_at IS NULL;
    IF n <> 7 THEN
        RAISE EXCEPTION 'expected 7 demo units below the root, found %', n;
    END IF;

    -- Every non-root unit type is exercised: that is what this tree is
    -- for. The root's type comes from the seed, not from here, so it is
    -- excluded -- counting it would pass or fail on the wrong thing.
    IF (SELECT COUNT(DISTINCT u.unit_type) FROM org.unit u
         WHERE u.uuid::text LIKE '5eed0000-0000-4000-a000-0000000002%'
           AND u.parent IS NOT NULL
           AND u.deleted_at IS NULL) < 3 THEN
        RAISE EXCEPTION 'demo tree no longer exercises Region, Branch and Locker';
    END IF;

    -- The tree hangs off the seeded root, whatever it is called.
    IF EXISTS (
        SELECT 1 FROM org.unit u
         WHERE u.code IN ('ERG', 'WRG')
           AND u.parent IS DISTINCT FROM (SELECT id FROM org.root())
    ) THEN
        RAISE EXCEPTION 'demo regions are not parented to the root';
    END IF;
END $$;

ROLLBACK;
