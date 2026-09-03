-- Deploy odo:004_registration_account_lockdown to pg
-- requires: 002_odo_seed

-- Switch the odo-registration machine account off by default.
--
-- 002_odo_seed originally created it active with the published password
-- 'odo-registration-dev-only'. That seed is fixed going forward, but it
-- has already been deployed, so this change remediates existing
-- databases: any install that ran the old seed is left with an active,
-- broadly-privileged account whose password is in the git history.
--
-- After this runs, the account cannot log in at all:
-- auth.verify_user_credentials() filters on status = 'active', so the
-- refusal happens in the database rather than the application layer, and
-- the password is replaced with a value nobody holds.
--
-- scripts/load-data-manifest.sh is what activates it for the duration of a
-- registration run. Dev and CI restore a known password afterwards from
-- src/test-data/ (which is re-applied on every deploy-test), so this is
-- safe to run there too.
--
-- Idempotent: re-running simply re-randomizes. On a fresh install the
-- seed has already left the account in this state and this is a no-op in
-- effect.

BEGIN;

UPDATE auth.usr
   SET status = 'inactive',
       updated_at = CURRENT_TIMESTAMP
 WHERE username = 'odo-registration';

UPDATE auth.local_account
   SET password_hash = crypt(gen_random_uuid()::text || gen_random_uuid()::text,
                             gen_salt('bf', 10)),
       failed_login_attempts = 0,
       locked_until = NULL,
       updated_at = CURRENT_TIMESTAMP
 WHERE usr = (SELECT id FROM auth.usr WHERE username = 'odo-registration');

COMMIT;
