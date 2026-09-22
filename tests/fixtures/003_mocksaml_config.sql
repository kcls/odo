-- MockSAML IdP + SP configuration for E2E SSO testing.
--
-- MockSAML (https://mocksaml.com) is a free SAML IdP for testing: it
-- accepts any password, so use 'test' for SSO logins. The SP keypair
-- below is a self-signed test certificate (CN odo-sp-test) with no
-- production value.
--
-- Idempotent: safe to re-run (upserts by entity_id / natural keys).

BEGIN;

-- MockSAML IdP
INSERT INTO auth.saml_idp_config (name, entity_id, sso_url, metadata_url, is_active)
SELECT 'MockSAML', 'https://saml.example.com/entityid',
       'https://mocksaml.com/api/saml/sso',
       'https://mocksaml.com/api/saml/metadata', true
 WHERE NOT EXISTS (SELECT 1 FROM auth.saml_idp_config
                    WHERE entity_id = 'https://saml.example.com/entityid');

-- IdP attributes: the SAML assertion fields odo-auth reads at login.
-- 'Location' resolves the user's working org unit; MockSAML sends
-- slash-joined values, hence the normalizer.
INSERT INTO auth.saml_idp_attribute (idp, key, label, is_location, normalizer)
SELECT i.id, v.key, v.label, v.is_location, v.normalizer
  FROM auth.saml_idp_config i,
       (VALUES
           ('Location', 'Location', true, 'split_slash_first'),
           ('Title', 'Job Title', false, NULL)
       ) AS v(key, label, is_location, normalizer)
 WHERE i.entity_id = 'https://saml.example.com/entityid'
   AND NOT EXISTS (SELECT 1 FROM auth.saml_idp_attribute a
                    WHERE a.idp = i.id AND a.key = v.key);

-- SP config for the vite dev server origin...
INSERT INTO auth.saml_sp_config
    (entity_id, acs_url, slo_url, callback_url, label, is_active, idp,
     metadata_url)
SELECT 'http://localhost:3001',
       'http://localhost:30080/saml/acs',
       'http://localhost:30080/saml/slo',
       'http://localhost:3001/login/callback',
       'MockSAML SSO', true, i.id,
       'https://mocksaml.com/api/saml/metadata'
  FROM auth.saml_idp_config i
 WHERE i.entity_id = 'https://saml.example.com/entityid'
ON CONFLICT (entity_id) WHERE is_active = true DO UPDATE SET
    acs_url = EXCLUDED.acs_url,
    slo_url = EXCLUDED.slo_url,
    callback_url = EXCLUDED.callback_url;

-- ...and for the containerized/k3s UI origin.
INSERT INTO auth.saml_sp_config
    (entity_id, acs_url, slo_url, callback_url, label, is_active, idp,
     metadata_url)
SELECT 'http://localhost:30080',
       acs_url, slo_url,
       'http://localhost:30080/login/callback',
       'MockSAML localhost:30080', true, idp,
       metadata_url
  FROM auth.saml_sp_config
 WHERE entity_id = 'http://localhost:3001'
-- Everything here is copied from the :3001 row, so a re-run has nothing
-- new to write. (Before odo:006_drop_sp_signing_material this refreshed
-- the SP cert and key, which were the only fields that could differ.)
ON CONFLICT (entity_id) WHERE is_active = true DO NOTHING;

COMMIT;
