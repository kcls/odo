-- Revert odo:003_directory_upload_mapping from pg

BEGIN;

DROP INDEX IF EXISTS asset.directory_upload_mapping_key;
ALTER TABLE asset.directory
    DROP CONSTRAINT IF EXISTS directory_category_requires_entity_type,
    DROP COLUMN IF EXISTS entity_type,
    DROP COLUMN IF EXISTS category;

COMMIT;
