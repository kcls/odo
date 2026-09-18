-- Verify odo:003_directory_upload_mapping on pg

BEGIN;

SELECT entity_type, category FROM asset.directory WHERE FALSE;
SELECT 1/COUNT(*) FROM pg_indexes
 WHERE schemaname = 'asset' AND indexname = 'directory_upload_mapping_key';

ROLLBACK;
