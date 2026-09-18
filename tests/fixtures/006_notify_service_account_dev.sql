-- Dev/CI credential for the odo-notify-service machine account.
--
-- The account ships with an unknowable password (002_odo_seed) so that a
-- production install has no usable credential for it until an operator
-- sets one. Dev clusters, the integration tests and Current's
-- draft-reminder job all expect to log in as it, so the known password
-- lives here with the rest of the dev fixtures rather than in the schema.
--
-- Unlike odo-registration there is no status to flip: this account stays
-- active in the seed, because background jobs authenticate as it
-- continuously rather than for the length of one run.
--
-- Consequence worth keeping in mind: any install that runs deploy-test
-- has an odo-notify-service account with a published password. That is
-- the point -- test data is for dev and CI. Production installs must
-- never load it, and should set the password with
-- scripts/manage-secrets.sh update-notify-service instead, which writes
-- the matching Kubernetes secret.
--
-- Keep in sync with NOTIFY_SERVICE_PASSWORD in
-- tests/integration/src/lib.rs.
--
-- Idempotent: safe to re-run.

BEGIN;

UPDATE auth.local_account
   SET password_hash = crypt('odo-notify-service-dev-only', gen_salt('bf', 10)),
       failed_login_attempts = 0,
       locked_until = NULL,
       updated_at = CURRENT_TIMESTAMP
 WHERE usr = (SELECT id FROM auth.usr WHERE username = 'odo-notify-service');

COMMIT;
