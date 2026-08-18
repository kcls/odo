-- Verify odo:001_odo_baseline on pg

BEGIN;

SELECT 1/COUNT(*) FROM information_schema.schemata WHERE schema_name = 'auth';
SELECT 1/COUNT(*) FROM information_schema.schemata WHERE schema_name = 'authz';
SELECT 1/COUNT(*) FROM information_schema.schemata WHERE schema_name = 'org';
SELECT 1/COUNT(*) FROM information_schema.schemata WHERE schema_name = 'asset';
SELECT 1/COUNT(*) FROM information_schema.schemata WHERE schema_name = 'notification';
SELECT id FROM auth.usr WHERE FALSE;

ROLLBACK;
