-- Verify odo:006_drop_sp_signing_material on pg

BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'auth' AND table_name = 'saml_sp_config'
           AND column_name IN ('private_key', 'x509_cert')
    ) THEN
        RAISE EXCEPTION 'saml_sp_config still has SP signing columns';
    END IF;

    -- The IdP's certificate is a different thing and must survive: it is
    -- what incoming assertions are verified against.
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'auth' AND table_name = 'saml_sp_config'
           AND column_name = 'idp_x509_cert'
    ) THEN
        RAISE EXCEPTION 'idp_x509_cert is missing -- it must not be dropped';
    END IF;
END $$;

ROLLBACK;
