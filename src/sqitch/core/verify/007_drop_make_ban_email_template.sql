-- Verify odo:007_drop_make_ban_email_template on pg

BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'notification' AND p.proname = 'make_ban_email_template'
    ) THEN
        RAISE EXCEPTION 'notification.make_ban_email_template still exists';
    END IF;
END $$;

ROLLBACK;
