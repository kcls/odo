-- Dev/CI credential for the odo-registration machine account.
--
-- The account ships disabled with an unknowable password (002_odo_seed,
-- 004_registration_account_lockdown) so that a production install has no
-- usable credential for it. Dev clusters, the integration tests and the
-- e2e suite all expect to log in as it directly, so the known password
-- lives here with the rest of the dev fixtures rather than in the schema.
--
-- Consequence worth keeping in mind: any install that runs deploy-test
-- has an active odo-registration account with a published password. That
-- is the point -- test data is for dev and CI. Production installs must
-- never load it, and should reach the account through
-- scripts/load-data-manifest.sh instead.
--
-- Keep in sync with REGISTRATION_PASSWORD in src/integration-tests/src/lib.rs.
--
-- Idempotent: safe to re-run.

BEGIN;

UPDATE auth.usr
   SET status = 'active',
       updated_at = CURRENT_TIMESTAMP
 WHERE username = 'odo-registration';

UPDATE auth.local_account
   SET password_hash = crypt('odo-registration-dev-only', gen_salt('bf', 10)),
       failed_login_attempts = 0,
       locked_until = NULL,
       updated_at = CURRENT_TIMESTAMP
 WHERE usr = (SELECT id FROM auth.usr WHERE username = 'odo-registration');

COMMIT;
