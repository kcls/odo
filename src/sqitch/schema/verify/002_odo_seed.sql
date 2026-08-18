-- Verify odo:002_odo_seed on pg

BEGIN;

SELECT 1/COUNT(*) FROM org.unit WHERE code = 'OLS';
SELECT 1/COUNT(*) FROM authz.role WHERE code = 'odo-admin';
SELECT 1/COUNT(*) FROM auth.usr WHERE username = 'odo-registration';

ROLLBACK;
