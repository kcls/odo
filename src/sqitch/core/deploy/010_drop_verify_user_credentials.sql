-- Deploy odo:010_drop_verify_user_credentials to pg
-- requires: 001_odo_baseline

-- Drop auth.verify_user_credentials(text, text).
--
-- Nothing calls it, and it can't run: it filters on auth.usr.is_deleted,
-- a column that doesn't exist. 002_odo_seed and
-- 004_registration_account_lockdown credit it with refusing login to an
-- inactive account; that check now lives in odo-auth itself (password
-- login, SAML login, and token refresh all require status = 'active').

BEGIN;

DROP FUNCTION auth.verify_user_credentials(text, text);

COMMIT;
