-- Verify odo:010_drop_verify_user_credentials on pg

BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'auth' AND p.proname = 'verify_user_credentials'
    ) THEN
        RAISE EXCEPTION 'auth.verify_user_credentials still exists';
    END IF;
END $$;

ROLLBACK;
