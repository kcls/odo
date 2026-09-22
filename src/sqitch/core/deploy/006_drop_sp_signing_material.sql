-- Deploy odo:006_drop_sp_signing_material to pg
-- requires: 001_odo_baseline

-- Drop the SP's own signing material: auth.saml_sp_config.private_key
-- and .x509_cert.
--
-- odo never signs anything. Incoming SAML responses are verified against
-- the *IdP's* certificate (idp_x509_cert, which stays), and no code path
-- ever loaded the SP private key -- it was written by the admin API and
-- read by nothing. The certificate had one consumer, the SP metadata
-- endpoint, which advertised a KeyDescriptor for a signing key that does
-- not exist; that endpoint is removed in the same release.
--
-- Verified operationally before removal: both columns were set to the
-- empty string on a live SSO install and login, logout and re-login
-- through the IdP were unaffected, across browsers.
--
-- Storing an unused private key is the part worth being rid of. A
-- credential that nothing reads is still a credential that leaks.

BEGIN;

ALTER TABLE auth.saml_sp_config
    DROP COLUMN private_key,
    DROP COLUMN x509_cert;

COMMIT;
