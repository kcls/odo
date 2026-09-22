-- Revert odo:006_drop_sp_signing_material from pg
--
-- The columns come back empty. Their contents are not recoverable here,
-- and should not be: the private key was a credential, so a revert that
-- restored one from a backup would be reintroducing it. An install that
-- genuinely needs SP signing sets them again through the admin API.
--
-- NOT NULL with an empty-string default, matching the original shape
-- without requiring a value nothing supplies.

BEGIN;

ALTER TABLE auth.saml_sp_config
    ADD COLUMN x509_cert text NOT NULL DEFAULT '',
    ADD COLUMN private_key text NOT NULL DEFAULT '';

COMMIT;
