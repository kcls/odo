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
     metadata_url, x509_cert, private_key)
SELECT 'http://localhost:3001',
       'http://localhost:30080/saml/acs',
       'http://localhost:30080/saml/slo',
       'http://localhost:3001/login/callback',
       'MockSAML SSO', true, i.id,
       'https://mocksaml.com/api/saml/metadata',
       '-----BEGIN CERTIFICATE-----
MIIDDTCCAfWgAwIBAgIUJiqnOEWbWuuzeMWY6Km16tKiePIwDQYJKoZIhvcNAQEL
BQAwFjEUMBIGA1UEAwwLb2RvLXNwLXRlc3QwHhcNMjYwODE2MTIwMDIxWhcNMjgw
ODE1MTIwMDIxWjAWMRQwEgYDVQQDDAtvZG8tc3AtdGVzdDCCASIwDQYJKoZIhvcN
AQEBBQADggEPADCCAQoCggEBALurSxxizSC1SJQdtsgNWXw3vbx0ehtV1zoQGDz6
79SctrJPFSGUIkOL2AwmoxBGq8PzyQhtOGewYdsUDyQr7+r9zj2+JPIrBWsnfW2A
55Fv7o/rs0w8Srr59XgQL7BAzsieDfL+V6jYDJOsG1JM7P9QlEt9vLvrrWEnkPh2
Sp2AdsIUQ4EUBaHmU4j63tbymGK6Yd1WH9//22PtuUmNN+PLka70TdW0XkEDd6NM
mqnnv7Le3oW7UVU7b+wAT6js9qSI3pIe0h5D9Q1XQA/BtoZYDdjoVtkqXcwsFHZ9
xHvOUUH0Wq1jHBYXWpcb8WBgl4y//VAuHEi/I/o4K5MeP18CAwEAAaNTMFEwHQYD
VR0OBBYEFCMPE5kooRUhHwZCnKIxTQoCZ5aBMB8GA1UdIwQYMBaAFCMPE5kooRUh
HwZCnKIxTQoCZ5aBMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEB
ABUL301qami24ThWaQkrZykCwxtlAFthVJ1ypCY2u52+cuvaPjxNdAC8DQIqTpkM
kAfZtNUfK3V1JAaL/GXmCyZRsUC2ZzJpvf+1aXaJdwS3k2uBR6T3uJP+b5YsUiAO
1OuhBMg6ZARyCjPr2ZhP0oLys1rBha3Fd1ruj+ECdf8O9waF6KpoxFkNhKvbowhk
4ogVcZkEAnQH2taiG5W+gDUCTlIBlCXtNVVpYcD0haXmfBQJ5Wp6l/gcSTjuj4S1
d9dKL5tXgHicfckK4XEhoKbT1LJjtsYQiMWudU8fR5yOk+BbqmKlGDvllB7KM1Y7
e+sf3kgClBmmbqcbFCsvmeU=
-----END CERTIFICATE-----',
       '-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQC7q0scYs0gtUiU
HbbIDVl8N728dHobVdc6EBg8+u/UnLayTxUhlCJDi9gMJqMQRqvD88kIbThnsGHb
FA8kK+/q/c49viTyKwVrJ31tgOeRb+6P67NMPEq6+fV4EC+wQM7Ing3y/leo2AyT
rBtSTOz/UJRLfby7661hJ5D4dkqdgHbCFEOBFAWh5lOI+t7W8phiumHdVh/f/9tj
7blJjTfjy5Gu9E3VtF5BA3ejTJqp57+y3t6Fu1FVO2/sAE+o7PakiN6SHtIeQ/UN
V0APwbaGWA3Y6FbZKl3MLBR2fcR7zlFB9FqtYxwWF1qXG/FgYJeMv/1QLhxIvyP6
OCuTHj9fAgMBAAECggEAEG6VP/HtgkJaDZ00CC+VcoxsGkIPZpeG2SHx1VxApc4A
9/tEeOSAQyGaZ0BcziVTj2+n99gEWwPbiM11U+/3uYyQKeGdWSNOMQ2Yn5ifutRh
P/KRCCK0hZf/fiWnEn77yOuzgfkmK/XOCButZ4pHxN7FbuS4CUyl9jGuMZGKwyzt
e+U7HM65LAFo338HqWLi+a8/bFIA3NN2UN142JKmKnyM65I4jq/jI/6Z+yXmxK6V
wXEdHkXo7CSqgPafXqEQzLH6/DL+Th97Ae9uC6oNJw/lt3pLoA19BWRfrJckEosK
bQ19Z+YrSYQJgFHuZxCUMnqoXe2AlboeDKDmcicKPQKBgQDev67E1M/JplqKWnpT
fqHamOT5EaVe2MJTWNBDjc+7KbeOKkJwDA5humI7Wd/qFqyKgviOkv9sq2s8ekkZ
/fuHpdvCl00J/brsivuqfGdbfYxZpaZOrp9Zdl2LMRAtJqX7ZfJN7LUx54B3+KBo
CaSMo1LL62KgioBaYi3yARu1gwKBgQDXrwvWMSlCDrrxN3bzNgveyiGqScujZllC
vJWOaTdXEV7p1Fk+CMzEUJ/fFsyPsknDXbIeECQqj1Kow895EOdHSpuL1UrYtpp/
Z8M3LiQDKRWDBT+eRG5axLIldO5KfiWer2dZTcNaCQCUl/xLzhCe6COlrCUgkBI6
w8/AnVMD9QKBgB+O3q7qS6oRFIDHgs8zgLDcuowDEP/YC+gNDCyV+dlVdrkAibsg
KiV0Z5hrCks6/ST+m0Jv1xpJSv8dgB/bmPhF6lWuY+7HcOU0Z6VmzKnspqbIzkAV
g2QEXgprYBRVhmyQq/yYTa+NUektY2R6AUMfnIphhe6i0L59bG798zQLAoGAaMhq
6LljgOslGRFIIapNJARxTIijfRO5I7n4soIdV5hh0xnN7VxbFrjQopIx+VG1kktP
wFk5KNAOaV0Py5JRugnd/ZY20YgNEP55EbLB3iM0hz2ihaJbNo++uIHRTrFwV2KB
xBoKYRBkjswzzyQiYQEIaHF0bBhyMsh0gvArp40CgYBQ7Kzm19Bl6ywZY/2HodiB
OD081WLrfM9BcvBa2nCLNJidKe5CoPDYKG2KP89ZROPO2K0L6faXcUlsFsIxOauO
44MhYXonYFZy5b4ZHsu8OvoELOejkh4FKs5kg6xm+EHkNxPV/haSpcHcxqJr2+iG
dEX+PERbujoNrHOXv2KYfg==
-----END PRIVATE KEY-----'
  FROM auth.saml_idp_config i
 WHERE i.entity_id = 'https://saml.example.com/entityid'
ON CONFLICT (entity_id) WHERE is_active = true DO UPDATE SET
    acs_url = EXCLUDED.acs_url,
    slo_url = EXCLUDED.slo_url,
    callback_url = EXCLUDED.callback_url,
    x509_cert = EXCLUDED.x509_cert,
    private_key = EXCLUDED.private_key;

-- ...and for the containerized/k3s UI origin.
INSERT INTO auth.saml_sp_config
    (entity_id, acs_url, slo_url, callback_url, label, is_active, idp,
     metadata_url, x509_cert, private_key)
SELECT 'http://localhost:30080',
       acs_url, slo_url,
       'http://localhost:30080/login/callback',
       'MockSAML localhost:30080', true, idp,
       metadata_url, x509_cert, private_key
  FROM auth.saml_sp_config
 WHERE entity_id = 'http://localhost:3001'
ON CONFLICT (entity_id) WHERE is_active = true DO UPDATE SET
    x509_cert = EXCLUDED.x509_cert,
    private_key = EXCLUDED.private_key;

COMMIT;
