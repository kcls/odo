-- Deploy odo:003_directory_upload_mapping to pg
-- requires: 001_odo_baseline

-- Move the upload-routing convention into the directory registry: a
-- directory row may declare that uploads with a given entity_type (and
-- optionally a specific category) land under its path. Apps register
-- their upload routing alongside the directory and its permissions,
-- instead of odo-asset hard-coding app entity types.
--
-- Rows with entity_type NULL are pure path-protection entries (no
-- upload routing). A NULL category is the entity_type's catch-all;
-- a non-NULL category is an exact match that wins over the catch-all.

BEGIN;

ALTER TABLE asset.directory
    ADD COLUMN entity_type TEXT,
    ADD COLUMN category TEXT,
    ADD CONSTRAINT directory_category_requires_entity_type
        CHECK (category IS NULL OR entity_type IS NOT NULL);

-- One mapping per (entity_type, category), with NULL category treated as
-- a distinct catch-all slot per entity_type.
CREATE UNIQUE INDEX directory_upload_mapping_key
    ON asset.directory (entity_type, COALESCE(category, ''))
    WHERE entity_type IS NOT NULL;

COMMIT;
